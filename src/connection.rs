use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::AppError;

const JSONRPC_VERSION: &str = "2.0";

#[derive(Debug, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
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
pub enum RequestParams {
    Plain {
        http_method: HttpMethod,
        endpoint: String,
        api_params: HashMap<String, serde_json::Value>,
    },
    HomeTimeline(HashMap<String, serde_json::Value>),
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
    pub result: ResponseResult,
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct ResponseResult {
    pub meta: ResponseResultMeta,
    pub body: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ResponseResultMeta {
    pub api_calls_remaining: usize,
    pub api_calls_reset: usize, // in epoch sec
}
