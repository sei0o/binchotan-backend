use crate::{
    api::HomeTimelineResponseBody,
    credential::CredentialStore,
    error::AppError,
    filter::{Filter, FilterError},
    methods::HttpMethod,
    models::Account,
    VERSION,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    io::Empty,
    path::PathBuf,
};
use thiserror::Error;
use tracing::{info, warn};

pub const JSONRPC_VERSION: &str = "2.0";

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    #[serde(flatten)]
    pub method: Method,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Method {
    #[serde(rename = "v0.plain")]
    Plain(PlainParams),
    #[serde(rename = "v0.home_timeline")]
    HomeTimeline(HomeTimelineParams),
    #[serde(rename = "v0.status")]
    Status(EmptyParams),
    #[serde(rename = "v0.account.list")]
    AccountList(AccountListParams),
    #[serde(rename = "v0.account.add")]
    AccountAdd(EmptyParams),
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlainParams {
    session_key: String,
    http_method: HttpMethod,
    endpoint: String,
    #[serde(default)]
    api_params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HomeTimelineParams {
    session_key: String,
    #[serde(default)]
    api_params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountListParams {
    session_key: String,
}

// TODO: ensure params are empty in a smarter way
#[derive(Debug, Clone, Deserialize)]
pub struct EmptyParams {
    #[serde(default)]
    params: HashMap<String, serde_json::Value>,
    #[serde(skip)]
    validated: RefCell<bool>,
}

impl EmptyParams {
    pub fn validate(&self) -> bool {
        let mut validated = self.validated.borrow_mut();
        *validated = true;
        self.params.is_empty()
    }
}

impl Drop for EmptyParams {
    fn drop(&mut self) {
        let validated = self.validated.borrow();
        if !*validated {
            unreachable!("EmptyParams must be validated (ensured that they are empty)")
        }
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
    AccountList {
        // Account id which the user used for authorization.
        owner: String,
        // Session keys for the owner account and accounts it owns.
        session_keys: HashMap<String, String>,
    },
    #[serde(rename = "result")]
    AccountAdd {
        user_id: String,
        session_key: String,
    },
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
            AppError::Listener(_) => unreachable!(),
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
    #[error("wrong parameters in request (id = {0})")]
    ParamsMismatch(String),
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
            Method::Plain(params) => self.handle_plain(req.id, params).await?,
            Method::HomeTimeline(params) => self.handle_timeline(req.id, params).await?,
            Method::Status(params) => self.handle_status(req.id, params).await?,
            Method::AccountList(params) => self.handle_account_list(req.id, params).await?,
            Method::AccountAdd(params) => self.handle_account_add(req.id, params).await?,
        };

        Ok(resp)
    }

    async fn handle_plain(&self, id: String, params: PlainParams) -> Result<Response, AppError> {
        let PlainParams {
            session_key,
            http_method,
            endpoint,
            api_params,
        } = params;

        let client = self.store.client_for(&session_key).await?;
        let api_params = serde_json::to_string(&api_params).map_err(HandlerError::ParamsParse)?;
        let (resp, remaining, reset) = client.call(&http_method, &endpoint, api_params).await?;
        info!("got response for plain request with id {}", id);

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
            id,
        })
    }

    async fn handle_timeline(
        &self,
        id: String,
        params: HomeTimelineParams,
    ) -> Result<Response, AppError> {
        let HomeTimelineParams {
            session_key,
            mut api_params,
        } = params;

        let client = self.store.client_for(&session_key).await?;
        let (
            HomeTimelineResponseBody {
                data: tweets,
                includes,
                meta,
            },
            remaining,
            reset,
        ) = client.timeline(&mut api_params).await?;
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
            id,
        })
    }

    async fn handle_status(
        &self,
        id: String,
        params: EmptyParams,
    ) -> Result<Response, HandlerError> {
        if !params.validate() {
            return Err(HandlerError::ParamsMismatch(id));
        }

        let content = ResponseContent::Status {
            version: VERSION.to_string(),
        };

        Ok(Response {
            jsonrpc: JSONRPC_VERSION.to_string(),
            content,
            id,
        })
    }

    async fn handle_account_list(
        &self,
        id: String,
        params: AccountListParams,
    ) -> Result<Response, AppError> {
        let AccountListParams { session_key } = params;
        let content = ResponseContent::AccountList {
            owner: self.store.id_for(&session_key).await?,
            session_keys: self.store.accounts(&session_key).await?,
        };

        Ok(Response {
            jsonrpc: JSONRPC_VERSION.to_string(),
            content,
            id,
        })
    }

    async fn handle_account_add(
        &mut self,
        id: String,
        params: EmptyParams,
    ) -> Result<Response, AppError> {
        if !params.validate() {
            return Err(HandlerError::ParamsMismatch(id).into());
        }

        let (user_id, session_key) = self.store.auth().await?;
        let content = ResponseContent::AccountAdd {
            user_id,
            session_key,
        };

        Ok(Response {
            jsonrpc: JSONRPC_VERSION.to_string(),
            content,
            id,
        })
    }
}
