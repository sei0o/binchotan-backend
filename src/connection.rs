use crate::{
    api::HomeTimelineResponseBody,
    credential::CredentialStore,
    error::AppError,
    filter::{Filter, FilterError},
    methods::HttpMethod,
    VERSION,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};
use thiserror::Error;
use tracing::{info, warn};

pub const JSONRPC_VERSION: &str = "2.0";

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub method: Method,
    #[serde(default)]
    pub params: RequestParams,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub enum Method {
    #[serde(rename = "v0.plain")]
    Plain,
    #[serde(rename = "v0.home_timeline")]
    HomeTimeline,
    #[serde(rename = "v0.status")]
    Status,
    #[serde(rename = "v0.account.list")]
    AccountList,
    #[serde(rename = "v0.account.add")]
    AccountAdd,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RequestParams {
    Plain {
        user_id: String,
        http_method: HttpMethod,
        endpoint: String,
        api_params: HashMap<String, serde_json::Value>,
    },
    MapWithId {
        user_id: String,
        api_params: HashMap<String, serde_json::Value>,
    },
    Map(HashMap<String, serde_json::Value>),
}

impl Default for RequestParams {
    fn default() -> Self {
        RequestParams::Map(HashMap::new())
    }
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub jsonrpc: String,
    #[serde(flatten)]
    pub content: ResponseContent,
    pub id: String,
}

#[derive(Debug, Serialize)]
pub enum ResponseContent {
    #[serde(rename = "result")]
    Plain {
        meta: ResponsePlainMeta,
        body: serde_json::Value,
    },
    #[serde(rename = "result")]
    HomeTimeline {
        meta: ResponsePlainMeta,
        body: HomeTimelineResponseBody,
    },
    #[serde(rename = "result")]
    Status { version: String },
    #[serde(rename = "result")]
    AccountList { user_ids: Vec<String> },
    #[serde(rename = "result")]
    AccountAdd { user_id: String },
    #[serde(rename = "error")]
    Error(ResponseError),
}

#[derive(Debug, Serialize)]
pub struct ResponsePlainMeta {
    pub api_calls_remaining: usize,
    pub api_calls_reset: usize, // in epoch sec
}

#[derive(Debug, Serialize)]
pub struct ResponseError {
    pub code: isize,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

enum RpcError {
    Parse,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    Internal,
    Server(RpcServerError),
}

impl From<RpcError> for isize {
    fn from(err: RpcError) -> Self {
        match err {
            RpcError::Parse => -32700,
            RpcError::InvalidRequest => -32600,
            RpcError::MethodNotFound => -32601,
            RpcError::InvalidParams => -32602,
            RpcError::Internal => -32603,
            RpcError::Server(c) => c.into(),
        }
    }
}

enum RpcServerError {
    Api,
    ApiStatus,
    Lua,
    Other,
}

impl From<RpcServerError> for isize {
    fn from(err: RpcServerError) -> Self {
        match err {
            RpcServerError::Api => -32000,
            RpcServerError::ApiStatus => -32001,
            RpcServerError::Lua => -32002,
            RpcServerError::Other => -32099,
        }
    }
}

// TODO: include concrete error types (CacheManager, ApiClient etc.) under HandlerErrors, and use HandlerErrors instead to get rid of unreachables?
impl From<AppError> for ResponseError {
    fn from(err: AppError) -> Self {
        let code = match err {
            AppError::Config(_) => unreachable!(),
            AppError::SocketSerialize(_) => unreachable!(),
            AppError::SocketBind(_) => unreachable!(),
            AppError::SocketPayloadParse(_) => unreachable!(),
            AppError::CacheManager(_) => RpcError::Server(RpcServerError::Other),
            AppError::CredentialStore(_) => RpcError::Server(RpcServerError::Other),
            AppError::Auth(_) => RpcError::Server(RpcServerError::Other),
            AppError::ApiClient(_) => RpcError::Server(RpcServerError::Other),
            AppError::Handler(ref e) => match e {
                HandlerError::ParamsParse(_) => RpcError::Parse,
                HandlerError::Version => RpcError::InvalidRequest,
                HandlerError::UnknownAccount(_) => RpcError::InvalidParams,
                HandlerError::ParamsMismatch(_) => RpcError::InvalidParams,
            },
            AppError::Filter(ref e) => match e {
                FilterError::PathNotDir(_) => RpcError::Server(RpcServerError::Other),
                FilterError::MetaParse(_) => RpcError::Server(RpcServerError::Other),
                FilterError::InsufficientScopes(_, _) => RpcError::Server(RpcServerError::Other),
                FilterError::Io(_) => RpcError::Server(RpcServerError::Other),
                FilterError::Lua(_) => RpcError::Server(RpcServerError::Lua),
            },
            AppError::Lua(_) => RpcError::Server(RpcServerError::Lua),
            AppError::Io(_) => RpcError::Server(RpcServerError::Other),
            AppError::Other(_) => RpcError::Server(RpcServerError::Other),
        }
        .into();

        ResponseError {
            code,
            message: err.to_string(),
            data: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum HandlerError {
    #[error("could not parse the parameters in the JSON-RPC request: {0}")]
    ParamsParse(serde_json::Error),
    #[error("incompatible JSON-RPC version. use 2.0 instead")]
    Version,
    #[error("unregistered user id: {0}")]
    UnknownAccount(String),
    #[error("wrong parameters in the JSON-RPC request for method {:?}: {:?}", .0.method, .0.params)]
    ParamsMismatch(Request),
}

pub struct Handler {
    pub store: CredentialStore,
    pub filter_path: PathBuf,
    pub scopes: HashSet<String>,
}

impl Handler {
    pub async fn handle(&mut self, req: Request) -> Response {
        let id = req.id.clone();
        match self.handle_inner(req).await {
            Ok(resp) => resp,
            Err(err) => {
                warn!("something bad happened: {:?}", err);
                let resp_err: ResponseError = err.into();
                Response {
                    jsonrpc: JSONRPC_VERSION.to_string(),
                    content: ResponseContent::Error(resp_err),
                    id,
                }
            }
        }
    }

    async fn handle_inner(&mut self, req: Request) -> Result<Response, AppError> {
        info!("received a request: {:?}", req);

        if req.jsonrpc.as_str() != JSONRPC_VERSION {
            return Err(HandlerError::Version.into());
        }

        let resp = match req.method {
            Method::Plain => self.handle_plain(req).await?,
            Method::HomeTimeline => self.handle_timeline(req).await?,
            Method::Status => self.handle_status(req).await?,
            Method::AccountList => self.handle_account_list(req).await?,
            Method::AccountAdd => self.handle_account_add(req).await?,
        };

        Ok(resp)
    }

    async fn handle_plain(&self, req: Request) -> Result<Response, AppError> {
        match req.params {
            RequestParams::Plain {
                user_id,
                http_method,
                endpoint,
                api_params,
            } => {
                let client = self.store.client_for(&user_id).await?;
                let api_params =
                    serde_json::to_string(&api_params).map_err(HandlerError::ParamsParse)?;
                let (resp, remaining, reset) =
                    client.call(&http_method, &endpoint, api_params).await?;
                info!("got response for plain request with id {}", req.id);

                let content = ResponseContent::Plain {
                    meta: ResponsePlainMeta {
                        api_calls_remaining: remaining,
                        api_calls_reset: reset,
                    },
                    body: resp,
                };
                Ok(Response {
                    jsonrpc: JSONRPC_VERSION.to_string(),
                    content,
                    id: req.id,
                })
            }
            _ => Err(HandlerError::ParamsMismatch(req).into()),
        }
    }

    async fn handle_timeline(&self, req: Request) -> Result<Response, AppError> {
        let (user_id, mut params) = match req.params {
            RequestParams::MapWithId {
                user_id,
                api_params,
            } => (user_id, api_params),
            _ => return Err(HandlerError::ParamsMismatch(req).into()),
        };
        let client = self.store.client_for(&user_id).await?;
        let (
            HomeTimelineResponseBody {
                data: tweets,
                includes,
                meta,
            },
            remaining,
            reset,
        ) = client.timeline(&mut params).await?;
        info!(
            "successfully retrieved {} tweets (reverse_chronological)",
            tweets.len(),
        );

        let filters = Filter::load(self.filter_path.as_ref(), &self.scopes)?;

        let mut filtered_tweets = vec![];
        'outer: for tweet in tweets {
            let mut result = tweet;
            for filter in &filters {
                match filter.run(&result)? {
                    Some(t) => result = t,
                    None => continue 'outer,
                }
            }
            filtered_tweets.push(result);
        }

        let content = ResponseContent::HomeTimeline {
            meta: ResponsePlainMeta {
                api_calls_remaining: remaining,
                api_calls_reset: reset,
            },
            body: HomeTimelineResponseBody {
                data: filtered_tweets,
                includes,
                meta,
            },
        };
        Ok(Response {
            jsonrpc: JSONRPC_VERSION.to_string(),
            content,
            id: req.id,
        })
    }

    async fn handle_status(&self, req: Request) -> Result<Response, HandlerError> {
        let req_ = req.clone();
        match req.params {
            RequestParams::Map(prms) => {
                if !prms.is_empty() {
                    return Err(HandlerError::ParamsMismatch(req_));
                }

                let content = ResponseContent::Status {
                    version: VERSION.to_string(),
                };

                Ok(Response {
                    jsonrpc: JSONRPC_VERSION.to_string(),
                    content,
                    id: req.id,
                })
            }
            _ => Err(HandlerError::ParamsMismatch(req)),
        }
    }

    async fn handle_account_list(&self, req: Request) -> Result<Response, AppError> {
        let req_ = req.clone();
        match req.params {
            RequestParams::Map(prms) => {
                if !prms.is_empty() {
                    return Err(HandlerError::ParamsMismatch(req_).into());
                }

                let content = ResponseContent::AccountList {
                    user_ids: self.store.user_ids(),
                };

                Ok(Response {
                    jsonrpc: JSONRPC_VERSION.to_string(),
                    content,
                    id: req.id,
                })
            }
            _ => Err(HandlerError::ParamsMismatch(req).into()),
        }
    }

    async fn handle_account_add(&mut self, req: Request) -> Result<Response, AppError> {
        let req_ = req.clone();
        match req.params {
            RequestParams::Map(prms) => {
                if !prms.is_empty() {
                    return Err(HandlerError::ParamsMismatch(req_).into());
                }

                let user_id = self.store.auth().await?;
                let content = ResponseContent::AccountAdd { user_id };

                Ok(Response {
                    jsonrpc: JSONRPC_VERSION.to_string(),
                    content,
                    id: req.id,
                })
            }
            _ => Err(HandlerError::ParamsMismatch(req).into()),
        }
    }
}
