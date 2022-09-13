use std::{cell::RefCell, collections::HashMap, path::PathBuf};

use thiserror::Error;
use tracing::info;

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
}

pub struct CredentialStore {
    cm: CacheManager,
    credentials: RefCell<HashMap<String, Credential>>,
    auth: Auth,
}

impl CredentialStore {
    pub fn new(cache_path: PathBuf, auth: Auth) -> Result<Self, CredentialStoreError> {
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
        })
    }

    pub fn user_ids(&self) -> Vec<String> {
        self.credentials.borrow_mut().keys().cloned().collect()
    }

    pub async fn client_for(&self, user_id: &str) -> Result<ApiClient, AppError> {
        let cred = {
            let cred = self.credentials.borrow_mut();
            cred.get(user_id).cloned()
        };
        match cred {
            Some(cred) => {
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
                            {
                                let mut creds = self.credentials.borrow_mut();
                                let cred = creds.get_mut(user_id).unwrap();
                                cred.state = CredentialState::Valid;
                                cred.access_token = acc.clone();
                                cred.refresh_token = refr;
                            }
                            self.save()?;
                            info!("successfully refreshed tokens");
                            let client = ApiClient::new(acc).await?;
                            return Ok(client);
                        }
                        Err(e) => return Err(e.into()),
                    };
                }

                unreachable!();
            }
            None => Err(CredentialStoreError::UnknownAccount(user_id.into()).into()),
        }
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
        let cred = Credential {
            access_token,
            refresh_token,
            state: CredentialState::Valid,
        };
        self.credentials
            .borrow_mut()
            .insert(client.user_id.clone(), cred);
        self.save()?;

        Ok(client.user_id)
    }

    pub fn save(&self) -> Result<(), AppError> {
        let creds = self.credentials.borrow_mut();
        self.cm.save(self.auth.scopes.clone(), creds.clone())
    }
}
