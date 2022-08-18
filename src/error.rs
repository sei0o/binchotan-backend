use std::path::PathBuf;

use thiserror::Error;

use crate::connection::Request;

/// AppError represents errors caused in this application.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("could not parse the cache file: {0}")]
    CacheParse(serde_json::Error),
    #[error("the environment variable was not defined: {0}")]
    EnvVar(#[from] std::env::VarError),
    #[error("could not parse the API response: {0}")]
    ApiResponseParse(serde_json::Error),
    #[error("field {0} was not found in the API response: {1:?}")]
    ApiResponseNotFound(String, serde_json::Value),
    #[error("could not convert the API response into JSON: {0}")]
    ApiResponseSerialize(serde_json::Error),
    #[error("failed to request the API: {0}")]
    ApiRequest(#[from] reqwest::Error),
    #[error("the API has given a non-successful status code ({0}): {1}")]
    ApiResponseStatus(u16, String),
    #[error("OAuth2 error: {0:?}")]
    OAuth(anyhow::Error),
    #[error("could not parse the URL: {0}")]
    OAuthUrlParse(#[from] url::ParseError),
    #[error("could not bind to the socket. another backend might be running? : {0}")]
    SocketBind(std::io::Error),
    #[error("could not parse the socket payload: {0}")]
    SocketPayloadParse(serde_json::Error),
    #[error("incompatible JSON-RPC version: {0}. use 2.0 instead")]
    JsonRpcVersion(String),
    #[error("could not parse the parameters in the JSON-RPC request: {0}")]
    JsonRpcParamsParse(serde_json::Error),
    #[error("wrong parameters in the JSON-RPC request for method {:?}: {:?}", .0.method, .0.params)]
    JsonRpcParamsMismatch(Request),
    #[error("too large payload")]
    JsonRpcTooLarge,
    #[error("the given path ({0}) is not a directory")]
    FilterPathNotDir(PathBuf),
    #[error("could not parse binchotan.toml")]
    FilterMetaParse(toml::de::Error),
    #[error("mlua error: {0}")]
    Lua(#[from] mlua::Error),
    #[error("other IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
