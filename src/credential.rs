use std::{cell::RefCell, collections::HashMap, path::PathBuf};

use sqlx::PgPool;
use thiserror::Error;
use tracing::info;
use uuid::Uuid;

use crate::{
    api::ApiClient,
    auth::Auth,
    cache::{Cache, CacheManager, CacheManagerError, Credential, CredentialState},
    error::AppError,
};

#[derive(Debug, Error)]
pub enum CredentialStoreError {
    #[error("unknown account: {0}")]
    UnknownAccount(String),
    #[error(transparent)]
    CacheManager(#[from] CacheManagerError),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub struct CredentialStore {
    cm: CacheManager,
    credentials: RefCell<HashMap<String, Credential>>,
    auth: Auth,
    conn: PgPool,
}

impl CredentialStore {
    pub fn new(
        cache_path: PathBuf,
        auth: Auth,
        conn: PgPool,
    ) -> Result<Self, CredentialStoreError> {
        let cm = CacheManager::new(cache_path);
        let Cache { accounts, scopes } = cm.load()?.unwrap_or_default();

        let credentials = if scopes == auth.scopes {
            accounts
        } else {
            HashMap::new()
        };

        Ok(Self {
            cm,
            auth,
            credentials: RefCell::new(credentials),
            conn,
        })
    }

    pub async fn id_for(&self, session_key: &str) -> Result<String, CredentialStoreError> {
        let rec = sqlx::query!(
            r#"
            select twitter_id from accounts where session_key = $1
            "#,
            session_key
        )
        .fetch_one(&self.conn)
        .await
        .map_err(|err| match err {
            sqlx::Error::RowNotFound => CredentialStoreError::UnknownAccount(session_key.into()),
            other => other.into(),
        })?;

        Ok(rec.twitter_id)
    }

    // Returns Twitter accounts (id and session key) available to the current user (the account which they were authenticated and ones they own).
    pub async fn accounts(
        &self,
        session_key: &str,
    ) -> Result<HashMap<String, String>, CredentialStoreError> {
        let accounts = sqlx::query!("select twitter_id, session_key from accounts where session_key = $1 or owned_by = (select id from accounts where session_key = $1)", session_key)
            .fetch_all(&self.conn)
            .await?
            .into_iter()
            // TODO: authenticate if needed
            .map(|rec| (rec.twitter_id, rec.session_key.unwrap_or("".to_owned())))
            .collect();

        Ok(accounts)
    }

    pub async fn client_for(&self, session_key: &str) -> Result<ApiClient, AppError> {
        let rec = sqlx::query!(
            r#"
            select * from accounts where session_key = $1
            "#,
            session_key
        )
        .fetch_one(&self.conn)
        .await
        .map_err(|err| match err {
            sqlx::Error::RowNotFound => CredentialStoreError::UnknownAccount(session_key.into()),
            other => other.into(),
        })?;

        let cred = Credential {
            access_token: rec.access_token,
            refresh_token: rec.refresh_token,
            state: CredentialState::Cached,
        };

        // TODO: avoid calling id_for_token on each api call
        let mut state = cred.state;
        if state == CredentialState::Cached {
            state = if ApiClient::validate_token(&cred.access_token).await? {
                CredentialState::Valid
            } else {
                CredentialState::Expired
            };
        }

        if state == CredentialState::Valid {
            info!("found valid token for {session_key}");
            match ApiClient::new(cred.access_token.clone()).await {
                Ok(client) => return Ok(client),
                Err(_) => state = CredentialState::Expired,
            }
        }

        if state == CredentialState::Expired {
            info!("found expired token for {session_key}, refreshing...");
            match self.auth.refresh_tokens(cred.refresh_token.clone()).await {
                Ok((acc, refr)) => {
                    sqlx::query!(
                        r#"
                        update accounts
                            set access_token = $1, refresh_token = $2
                            where session_key = $3
                    "#,
                        acc,
                        refr,
                        session_key
                    )
                    .execute(&self.conn)
                    .await
                    .map_err(CredentialStoreError::Database)?;

                    info!("successfully refreshed tokens");
                    let client = ApiClient::new(acc).await?;
                    return Ok(client);
                }
                Err(e) => return Err(e.into()),
            };
        }

        unreachable!();
    }

    pub async fn auth(&mut self, owner_key: Option<String>) -> Result<(String, String), AppError> {
        let (acc, refr) = self.auth.generate_tokens().await?;
        let (user_id, session_key) = self.add_credential(acc, refr, owner_key).await?;

        Ok((user_id, session_key))
    }

    pub async fn add_credential(
        &mut self,
        access_token: String,
        refresh_token: String,
        owner_key: Option<String>,
    ) -> Result<(String, String), AppError> {
        let client = ApiClient::new(access_token.clone()).await?;
        let session_key = Uuid::new_v4().to_string();

        let owner_id = match owner_key {
            Some(key) => sqlx::query!(
                r#"
                    select id from accounts where session_key = $1
                "#,
                key
            )
            .fetch_one(&self.conn)
            .await
            .map(|rec| rec.id)
            .map(Some)
            .map_err(maybe_notfound(key))?,
            None => None,
        };

        sqlx::query!(
            r#"
            insert into accounts
                (twitter_id, access_token, refresh_token, session_key, owned_by)
            values ($1, $2, $3, $4, $5)
            on conflict (twitter_id) do
                update set access_token = $2, refresh_token = $3, session_key = $4, owned_by = $5
            "#,
            client.user_id,
            access_token,
            refresh_token,
            session_key,
            owner_id
        )
        .execute(&self.conn)
        .await
        .map_err(CredentialStoreError::Database)?;

        Ok((client.user_id, session_key))
    }
}

fn maybe_notfound(session_key: String) -> Box<dyn Fn(sqlx::Error) -> CredentialStoreError> {
    let key = session_key.clone();
    Box::new(move |err| match err {
        sqlx::Error::RowNotFound => CredentialStoreError::UnknownAccount(key.clone()),
        other => other.into(),
    })
}
