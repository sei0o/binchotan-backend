use thiserror::Error;

/// AppError represents errors caused in this application.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("the environment variable was not defined")]
    EnvVar(#[from] std::env::VarError),
    #[error("reached rate limit for Twitter API")]
    ApiRateLimit,
    #[error("could not parse the API response")]
    ApiResponseParse(serde_json::Error),
    #[error("field {0} was not found in the API response")]
    ApiResponseNotFound(String),
    #[error("failed to request the API")]
    ApiRequest(#[from] reqwest::Error),
    #[error("OAuth2 error: {0:?}")]
    OAuth(#[from] anyhow::Error),
    #[error("could not parse the URL")]
    OAuthUrlParse(#[from] url::ParseError),
    #[error("could not bind to the socket")]
    SocketBind(#[from] std::io::Error),
    #[error("could not parse the socket payload")]
    SocketPayloadParse(serde_json::Error),
    #[error("incompatible JSON-RPC version: {0}. use 2.0 instead")]
    JsonRpcVersion(String),
    #[error("could not parse the parameters in the request")]
    JsonRpcParamsParse(serde_json::Error),
}
