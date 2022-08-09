use thiserror::Error;

/// AppError represents errors caused in this application.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("reached rate limit for Twitter API")]
    ApiRateLimit,
    #[error("could not parse the API response")]
    ApiResponseParse(#[from] serde_json::Error),
    #[error("field {0} was not found in the API response")]
    ApiResponseNotFound(String),
    #[error("failed to request the API")]
    ApiRequest(#[from] reqwest::Error),
    #[error("OAuth2 error: {0:?}")]
    OAuth(#[from] anyhow::Error),
    #[error("could not parse the URL")]
    OAuthUrlParse(#[from] url::ParseError),
}
