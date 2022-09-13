use crate::{
    api::ApiClientError, auth::AuthError, cache::CacheManagerError, connection::HandlerError,
    credential::CredentialStoreError, filter::FilterError,
};
use thiserror::Error;

/// AppError represents errors caused in this application.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("could not load configuration: {0}")]
    Config(#[from] config::ConfigError),
    #[error("could not convert the API response into JSON: {0}")]
    SocketSerialize(serde_json::Error),
    #[error("could not bind to the socket. another backend might be running? : {0}")]
    SocketBind(std::io::Error),
    #[error("could not parse the socket payload: {0}")]
    SocketPayloadParse(serde_json::Error),
    #[error("cache manager error")]
    CacheManager(#[from] CacheManagerError),
    #[error("cred store error")]
    CredentialStore(#[from] CredentialStoreError),
    #[error("auth error")]
    Auth(#[from] AuthError),
    #[error("api client error")]
    ApiClient(#[from] ApiClientError),
    #[error("handler error")]
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
