use crate::{
    api::ApiClientError, auth::AuthError, cache::CacheManagerError, connection::HandlerError,
    credential::CredentialStoreError, filter::FilterError, ListenerError,
};
use thiserror::Error;

/// AppError represents errors caused in this application.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("could not load configuration: {0}")]
    Config(#[from] config::ConfigError),
    #[error("listener error")]
    Listener(#[from] ListenerError),
    #[error("cache manager error")]
    CacheManager(#[from] CacheManagerError),
    #[error("cred store error: {0}")]
    CredentialStore(#[from] CredentialStoreError),
    #[error("auth error")]
    Auth(#[from] AuthError),
    #[error("api client error")]
    ApiClient(#[from] ApiClientError),
    #[error("handler error: {0}")]
    Handler(#[from] HandlerError),
    #[error("filter error: {0}")]
    Filter(#[from] FilterError),
    #[error("mlua error: {0}")]
    Lua(#[from] mlua::Error),
    #[error("other IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
