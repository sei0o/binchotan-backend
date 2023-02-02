use crate::methods::HttpMethod;
use crate::tweet::Tweet;
use anyhow::anyhow;
use reqwest::header::CONTENT_TYPE;
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing::debug;

// TODO: use a crate dedicated for the twitter api?

#[derive(Debug, Serialize, Deserialize)]
pub struct HomeTimelineResponseBody {
    pub data: Vec<Tweet>,
    pub includes: Option<serde_json::Value>,
    pub meta: serde_json::Value,
}

#[derive(Debug, Error)]
pub enum ApiClientError {
    #[error("token for user id {0:?} has expired")]
    TokenExpired(Option<String>),
    #[error("could not find the API header: {0}")]
    RespHeader(anyhow::Error),
    #[error("could not parse the API response: {0}")]
    RespParse(serde_json::Error),
    #[error("field {0} was not found in the API response: {1:?}")]
    RespParamNotFound(String, serde_json::Value),
    #[error("the API has given a non-successful status code ({0}): {1}")]
    RespStatus(u16, String),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
}

pub struct ApiClient {
    client: Client,
    pub user_id: String,
    access_token: String,
}

impl ApiClient {
    pub async fn new(access_token: String) -> Result<Self, ApiClientError> {
        let client = Client::new();
        let user_id = Self::id_for_token(&client, &access_token).await?;

        Ok(Self {
            client,
            user_id,
            access_token,
        })
    }

    pub async fn validate_token(access_token: &str) -> Result<bool, ApiClientError> {
        let client = Client::new();
        match Self::id_for_token(&client, access_token).await {
            Ok(_id) => Ok(true),
            Err(ApiClientError::TokenExpired(_)) => Ok(false),
            Err(other) => Err(other),
        }
    }

    async fn id_for_token(client: &Client, access_token: &str) -> Result<String, ApiClientError> {
        let endpoint = "https://api.twitter.com/2/users/me";
        tracing::warn!("access token: {}", access_token);
        let resp = client
            .get(endpoint)
            .bearer_auth(access_token.to_owned())
            .send()
            .await?;
        let status = resp.status();
        let json = resp.text().await?;
        match status {
            x if x.is_success() => {}
            StatusCode::UNAUTHORIZED => return Err(ApiClientError::TokenExpired(None)),
            other => return Err(ApiClientError::RespStatus(other.as_u16(), json)),
        }

        let user_data: serde_json::Value =
            serde_json::from_str(&json).map_err(ApiClientError::RespParse)?;
        let id = user_data["data"]["id"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| ApiClientError::RespParamNotFound("id".into(), user_data))?;
        Ok(id)
    }

    /// Calls `users/:id/timelines/reverse_chronological` endpoint to fetch the home timeline of the user. Returns the response body, the remaining calls (`x-rate-limit-remaining`), and the end of the current rate-limiting time window in epoch seconds (`x-rate-limit-reset`), in this order.
    pub async fn timeline(
        &self,
        params: &mut HashMap<String, serde_json::Value>,
    ) -> Result<(HomeTimelineResponseBody, usize, usize), ApiClientError> {
        let endpoint = format!(
            "https://api.twitter.com/2/users/{}/timelines/reverse_chronological",
            self.user_id
        );

        let resp = self
            .client
            .get(endpoint)
            .query(params)
            .bearer_auth(self.access_token.to_owned())
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await?;

        let remaining = Self::get_header(&resp, "x-rate-limit-remaining")
            .map_err(ApiClientError::RespHeader)?;
        let reset =
            Self::get_header(&resp, "x-rate-limit-reset").map_err(ApiClientError::RespHeader)?;

        let status = resp.status();
        let json = resp.text().await?;
        match status {
            x if x.is_success() => {
                let content: serde_json::Value =
                    serde_json::from_str(&json).map_err(ApiClientError::RespParse)?;
                debug!("{:?}", content);
                let body: HomeTimelineResponseBody =
                    serde_json::value::from_value(content).map_err(ApiClientError::RespParse)?;
                Ok((body, remaining, reset))
            }
            x => Err(ApiClientError::RespStatus(x.as_u16(), json)),
        }
    }

    /// Calls an arbitrary endpoint with the method and the parameters given in the arguments. Path parameters such as `:id` are replace with those of the authenticating user. Returns the response body, the remaining calls (`x-rate-limit-remaining`), and the end of the current rate-limiting time window in epoch seconds (`x-rate-limit-reset`), in this order.
    pub async fn call(
        &self,
        method: &HttpMethod,
        endpoint_path: &str,
        body: String,
    ) -> Result<(serde_json::Value, usize, usize), ApiClientError> {
        let path = endpoint_path.replace(":id", &self.user_id);
        let endpoint = format!("https://api.twitter.com/2/{}", path);
        let resp = self
            .client
            .request(reqwest::Method::from(*method), endpoint)
            .body(body)
            .bearer_auth(self.access_token.to_owned())
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await?;
        let status = resp.status();

        let remaining = Self::get_header(&resp, "x-rate-limit-remaining")
            .map_err(ApiClientError::RespHeader)?;
        let reset =
            Self::get_header(&resp, "x-rate-limit-reset").map_err(ApiClientError::RespHeader)?;
        let json = resp.text().await?;

        match status {
            x if x.is_success() => {
                let val: serde_json::Value =
                    serde_json::from_str(&json).map_err(ApiClientError::RespParse)?;
                debug!("{:?}", val);
                Ok((val, remaining, reset))
            }
            x => Err(ApiClientError::RespStatus(x.as_u16(), json)),
        }
    }

    fn get_header(resp: &Response, key: &str) -> Result<usize, anyhow::Error> {
        let value = resp.headers().get(key).ok_or(anyhow!("no header"))?;
        let st = value.to_str()?;
        let num = st.parse::<usize>()?;
        Ok(num)
    }
}
