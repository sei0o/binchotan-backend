use std::{cell::RefCell, collections::HashMap, path::PathBuf};

use sqlx::{PgPool, Postgres};
use thiserror::Error;
use tracing::info;
use uuid::Uuid;

use crate::{
    api::ApiClient,
    auth::Auth,
    cache::{Cache, CacheManager, CacheManagerError, Credential, CredentialState},
    error::AppError,
    models::Account,
};

#[derive(Debug, Error)]
pub enum CredentialStoreError {
    #[error("unknown account: {0}")]
    UnknownAccount(String),
    #[error(transparent)]
    CacheManager(#[from] CacheManagerError),
    #[error(transparent)]
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

    // Returns Twitter account IDs owned by the current user.
    pub async fn user_ids(&self) -> Result<Vec<String>, CredentialStoreError> {
        // TODO: authenticate
        let ids = sqlx::query!("select twitter_id from accounts")
            .fetch_all(&self.conn)
            .await?
            .into_iter()
            .map(|rec| rec.twitter_id)
            .collect();

        Ok(ids)
    }

    pub async fn client_for(&self, user_id: &str) -> Result<ApiClient, AppError> {
        let rec = sqlx::query!(
            r#"
            select * from accounts where twitter_id = $1
            "#,
            user_id
        )
        .fetch_one(&self.conn)
        .await
        .map_err(|err| match err {
            sqlx::Error::RowNotFound => CredentialStoreError::UnknownAccount(user_id.into()),
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
            info!("found valid token for {user_id}");
            match ApiClient::new(cred.access_token.clone()).await {
                Ok(client) => return Ok(client),
                Err(_) => state = CredentialState::Expired,
            }
        }

        if state == CredentialState::Expired {
            info!("found expired token for {user_id}, refreshing...");
            match self.auth.refresh_tokens(cred.refresh_token.clone()).await {
                Ok((acc, refr)) => {
                    sqlx::query!(
                        r#"
                        update accounts
                            set access_token = $1, refresh_token = $2
                            where twitter_id = $3
                    "#,
                        acc,
                        refr,
                        user_id
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

    pub async fn auth(&mut self) -> Result<String, AppError> {
        let (acc, refr) = self.auth.generate_tokens().await?;
        let user_id = self.add_credential(acc, refr).await?;

        Ok(user_id)
    }

    pub async fn add_credential(
        &mut self,
        access_token: String,
        refresh_token: String,
    ) -> Result<String, AppError> {
        let client = ApiClient::new(access_token.clone()).await?;

        sqlx::query!(
            r#"
            insert into accounts
                (twitter_id, access_token, refresh_token, session_key)
            values ($1, $2, $3, $4)
            on conflict (twitter_id) do
                update set access_token = $2, refresh_token = $3, session_key = $4
            "#,
            client.user_id,
            access_token,
            refresh_token,
            Uuid::new_v4().to_string()
        )
        .fetch_one(&self.conn)
        .await
        .map_err(CredentialStoreError::Database)?;

        Ok(client.user_id)
    }
}
