use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use tracing::info;

use crate::{api, error::AppError, VERSION};

const JSONRPC_VERSION: &str = "2.0";

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
pub enum Method {
    #[serde(rename = "v0.plain")]
    Plain,
    #[serde(rename = "v0.home_timeline")]
    HomeTimeline,
    #[serde(rename = "v0.status")]
    Status,
}

#[derive(Debug, Default, Deserialize)]
pub enum RequestParams {
    Plain {
        http_method: HttpMethod,
        endpoint: String,
        api_params: HashMap<String, serde_json::Value>,
    },
    Map(HashMap<String, serde_json::Value>),
    #[default]
    Empty,
}

// We define an enum for HTTP request method since http::Method does not implement serde::Deserialize
#[derive(Debug, Deserialize)]
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

#[derive(Debug, Serialize)]
pub struct Response {
    pub jsonrpc: String,
    #[serde(flatten)]
    pub content: ResponseContent,
    pub id: String,
}

#[derive(Debug, Serialize)]
pub enum ResponseContent {
    Plain {
        meta: ResponsePlainMeta,
        body: serde_json::Value,
    },
    HomeTimeline {
        meta: ResponsePlainMeta,
        body: serde_json::Value,
    },
    Status {
        version: String,
    },
    Error(ResponseError),
}

#[derive(Debug, Serialize)]
pub struct ResponsePlainMeta {
    pub api_calls_remaining: usize,
    pub api_calls_reset: usize, // in epoch sec
}

#[derive(Debug, Serialize)]
pub struct ResponseError {
    pub code: usize,
    pub message: String,
    pub data: Option<serde_json::Value>,
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
                todo!()
            }
            _ => Err(AppError::JsonRpcParamsMismatch(req)),
        },
        Method::HomeTimeline => match req.params {
            RequestParams::Map(api_params) => {
                let tweets = client.timeline(&api_params).await?;
                info!(
                    "successfully retrieved tweets (reverse_chronological): {:?}",
                    tweets[0]
                );

                let resp = Response {
                    jsonrpc: JSONRPC_VERSION.to_string(),
                    content: todo!(),
                    id: req.id,
                };
            }
            _ => Err(AppError::JsonRpcParamsMismatch(req)),
        },
        Method::Status => match req.params {
            RequestParams::Empty => {
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
        },
    }
}
