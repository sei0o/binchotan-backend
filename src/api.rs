use std::collections::HashMap;

use anyhow::{anyhow, Context};
use reqwest::header::CONTENT_TYPE;
use reqwest::{Client, Response, StatusCode};
use tracing::debug;

use crate::error::AppError;
use crate::methods::HttpMethod;
use crate::tweet::Tweet;

// TODO: use a crate dedicated for the twitter api?
// TODO: pagination

pub struct ApiClient {
    client: Client,
    pub user_id: String,
    access_token: String,
}

impl ApiClient {
    pub async fn new(access_token: String) -> Result<Self, AppError> {
        let client = Client::new();
        let user_id = Self::id_for_token(&client, &access_token).await?;

        Ok(Self {
            client,
            user_id,
            access_token,
        })
    }

    pub async fn validate_token(access_token: &str) -> Result<bool, AppError> {
        let client = Client::new();
        match Self::id_for_token(&client, access_token).await {
            Ok(_id) => Ok(true),
            Err(AppError::TokenExpired(_)) => Ok(false),
            Err(other) => Err(other),
        }
    }

    async fn id_for_token(client: &Client, access_token: &str) -> Result<String, AppError> {
        let endpoint = "https://api.twitter.com/2/users/me";
        let resp = client
            .get(endpoint)
            .bearer_auth(access_token.to_owned())
            .send()
            .await?;
        let status = resp.status();
        let json = resp.text().await?;
        match status {
            x if x.is_success() => {}
            StatusCode::UNAUTHORIZED => return Err(AppError::TokenExpired(None)),
            other => return Err(AppError::ApiResponseStatus(other.as_u16(), json)),
        }

        let user_data: serde_json::Value =
            serde_json::from_str(&json).map_err(AppError::ApiResponseParse)?;
        let id = user_data["data"]["id"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| AppError::ApiResponseNotFound("id".to_owned(), user_data))?;
        Ok(id)
    }

    /// Calls `users/:id/timelines/reverse_chronological` endpoint to fetch the home timeline of the user. Returns the response body, the remaining calls (`x-rate-limit-remaining`), and the end of the current rate-limiting time window in epoch seconds (`x-rate-limit-reset`), in this order.
    pub async fn timeline(
        &self,
        params: &mut HashMap<String, serde_json::Value>,
    ) -> Result<(Vec<Tweet>, usize, usize), AppError> {
        let endpoint = format!(
            "https://api.twitter.com/2/users/{}/timelines/reverse_chronological",
            self.user_id
        );

        let tweet_fields = vec![
            "attachments",
            "author_id",
            "context_annotations",
            "conversation_id",
            "created_at",
            "entities",
            "geo",
            "id",
            "in_reply_to_user_id",
            "lang",
            //"non_public_metrics",
            "public_metrics",
            //"organic_metrics",
            //"promoted_metrics",
            "possibly_sensitive",
            "referenced_tweets",
            "reply_settings",
            "source",
            "text",
            "withheld",
        ];
        params.insert(
            "tweet.fields".to_string(),
            serde_json::json!(tweet_fields.join(",")),
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
            .map_err(AppError::ApiResponseHeader)?;
        let reset =
            Self::get_header(&resp, "x-rate-limit-reset").map_err(AppError::ApiResponseHeader)?;

        let status = resp.status();
        let json = resp.text().await?;
        match status {
            x if x.is_success() => {
                let content: serde_json::Value =
                    serde_json::from_str(&json).map_err(AppError::ApiResponseParse)?;
                debug!("{:?}", content);
                let data = &content["data"];
                let tweets: Vec<Tweet> = serde_json::value::from_value(data.clone())
                    .map_err(AppError::ApiResponseParse)?;
                Ok((tweets, remaining, reset))
            }
            x => Err(AppError::ApiResponseStatus(x.as_u16(), json)),
        }
    }

    /// Calls an arbitrary endpoint with the method and the parameters given in the arguments. Path parameters such as `:id` are replace with those of the authenticating user. Returns the response body, the remaining calls (`x-rate-limit-remaining`), and the end of the current rate-limiting time window in epoch seconds (`x-rate-limit-reset`), in this order.
    pub async fn call(
        &self,
        method: &HttpMethod,
        endpoint_path: &str,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<(serde_json::Value, usize, usize), AppError> {
        let path = endpoint_path.replace(":id", &self.user_id);
        let endpoint = format!("https://api.twitter.com/2/{}", path);
        let body = serde_json::to_string(params).map_err(AppError::RpcParamsParse)?;
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
            .map_err(AppError::ApiResponseHeader)?;
        let reset =
            Self::get_header(&resp, "x-rate-limit-reset").map_err(AppError::ApiResponseHeader)?;
        let json = resp.text().await?;

        match status {
            x if x.is_success() => {
                let val: serde_json::Value =
                    serde_json::from_str(&json).map_err(AppError::ApiResponseParse)?;
                debug!("{:?}", val);
                Ok((val, remaining, reset))
            }
            x => Err(AppError::ApiResponseStatus(x.as_u16(), json)),
        }
    }

    fn get_header(resp: &Response, key: &str) -> Result<usize, anyhow::Error> {
        let value = resp.headers().get(key).context("header not found")?;
        let st = value.to_str().context("invalid header")?;
        let num = st.parse::<usize>().context("invalid header")?;
        Ok(num)
    }
}
