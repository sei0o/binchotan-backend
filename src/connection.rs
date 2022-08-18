use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, error::Error, path::Path};
use tracing::info;

use crate::{api, error::AppError, filter::Filter, tweet::Tweet, VERSION};

pub const JSONRPC_VERSION: &str = "2.0";

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub method: Method,
    #[serde(default)]
    pub params: RequestParams,
    pub id: String,
}

impl Request {
    pub fn validate(&self) -> Result<(), AppError> {
        match self.jsonrpc.as_str() {
            JSONRPC_VERSION => Ok(()),
            v => Err(AppError::JsonRpcVersion(v.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub enum Method {
    #[serde(rename = "v0.plain")]
    Plain,
    #[serde(rename = "v0.home_timeline")]
    HomeTimeline,
    #[serde(rename = "v0.status")]
    Status,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RequestParams {
    Plain {
        http_method: HttpMethod,
        endpoint: String,
        api_params: HashMap<String, serde_json::Value>,
    },
    Map(HashMap<String, serde_json::Value>),
}

impl Default for RequestParams {
    fn default() -> Self {
        RequestParams::Map(HashMap::new())
    }
}

// We define an enum for HTTP request method since http::Method does not implement serde::Deserialize
#[derive(Debug, Clone, Copy, Deserialize)]
pub enum HttpMethod {
    #[serde(rename = "GET")]
    Get,
    #[serde(rename = "POST")]
    Post,
    #[serde(rename = "PUT")]
    Put,
    #[serde(rename = "DELETE")]
    Delete,
    // Twitter API does not utilize other methods
}

impl From<HttpMethod> for reqwest::Method {
    fn from(h: HttpMethod) -> Self {
        match h {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Delete => reqwest::Method::DELETE,
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
        body: Vec<Tweet>,
    },
    #[serde(rename = "result")]
    Status { version: String },
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

impl From<AppError> for ResponseError {
    fn from(err: AppError) -> Self {
        let code = match err {
            AppError::Io(_) => -32000,
            AppError::CacheParse(_) => -32000,
            AppError::EnvVar(_) => -32000,
            AppError::ApiResponseParse(_) => -32000,
            AppError::ApiResponseNotFound(_, _) => -32000,
            AppError::ApiResponseSerialize(_) => -32000,
            AppError::ApiRequest(_) => -32000,
            AppError::ApiResponseStatus(_, _) => -32001,
            AppError::OAuth(_) => -32000,
            AppError::OAuthUrlParse(_) => -32000,
            AppError::SocketBind(_) => -32000,
            AppError::SocketPayloadParse(_) => -32700,
            AppError::JsonRpcVersion(_) => -32600,
            AppError::JsonRpcParamsParse(_) => -32700,
            AppError::JsonRpcParamsMismatch(_) => -32602,
            AppError::JsonRpcTooLarge => -32603,
            AppError::FilterPathNotDir(_) => -32000,
            AppError::FilterMetaParse(_) => -32000,
            AppError::Lua(_) => -32002,
            AppError::Other(_) => -32099,
        };

        ResponseError {
            code,
            message: err.to_string(),
            data: None,
        }
    }
}

pub async fn handle_request(req: Request, client: &api::ApiClient) -> Result<Response, AppError> {
    info!("received a request: {:?}", req);
    req.validate()?;

    match req.method {
        Method::Plain => match req.params {
            RequestParams::Plain {
                http_method,
                endpoint,
                api_params,
            } => {
                let resp = client.call(&http_method, &endpoint, &api_params).await?;
                info!("got response for plain request with id {}", req.id);

                let content = ResponseContent::Plain {
                    meta: ResponsePlainMeta {
                        // TODO:
                        api_calls_remaining: 0,
                        api_calls_reset: 0,
                    },
                    body: resp,
                };
                Ok(Response {
                    jsonrpc: JSONRPC_VERSION.to_string(),
                    content,
                    id: req.id,
                })
            }
            _ => Err(AppError::JsonRpcParamsMismatch(req)),
        },
        Method::HomeTimeline => {
            let params = match req.params {
                RequestParams::Map(api_params) => api_params,
                _ => return Err(AppError::JsonRpcParamsMismatch(req)),
            };
            let tweets = client.timeline(&params).await?;
            info!(
                "successfully retrieved {} tweets (reverse_chronological). here's one of them: {:?}", tweets.len(), tweets[0]
            );

            let filter_dir = env::var("FILTER_DIR")?;
            let path = Path::new(&filter_dir);
            let filters = Filter::load(path)?;

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
                    // TODO:
                    api_calls_remaining: 0,
                    api_calls_reset: 0,
                },
                body: filtered_tweets,
            };
            Ok(Response {
                jsonrpc: JSONRPC_VERSION.to_string(),
                content,
                id: req.id,
            })
        }
        Method::Status => {
            let req_ = req.clone();
            match req.params {
                RequestParams::Map(prms) => {
                    if !prms.is_empty() {
                        return Err(AppError::JsonRpcParamsMismatch(req_));
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
                _ => Err(AppError::JsonRpcParamsMismatch(req)),
            }
        }
    }
}
