use std::collections::HashMap;

use reqwest::header::CONTENT_TYPE;
use reqwest::{Client, StatusCode};
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

    /// Calls `users/:id/timelines/reverse_chronological` endpoint to fetch the home timeline of the user.
    pub async fn timeline(
        &self,
        params: &mut HashMap<String, serde_json::Value>,
    ) -> Result<Vec<Tweet>, AppError> {
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

        let json = self
            .client
            .get(endpoint)
            .query(params)
            .bearer_auth(self.access_token.to_owned())
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await?
            .text()
            .await?;
        let resp: serde_json::Value =
            serde_json::from_str(&json).map_err(AppError::ApiResponseParse)?;
        debug!("{:?}", resp);
        let data = &resp["data"];
        let tweets: Vec<Tweet> =
            serde_json::value::from_value(data.clone()).map_err(AppError::ApiResponseParse)?;
        Ok(tweets)
    }

    /// Calls an arbitrary endpoint with the method and the parameters given in the arguments. Path parameters such as `:id` are replace with those of the authenticating user.
    pub async fn call(
        &self,
        method: &HttpMethod,
        endpoint_path: &str,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, AppError> {
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
        let json = resp.text().await?;

        match status {
            x if x.is_success() => {
                let val: serde_json::Value =
                    serde_json::from_str(&json).map_err(AppError::ApiResponseParse)?;
                debug!("{:?}", val);
                Ok(val)
            }
            x => Err(AppError::ApiResponseStatus(x.as_u16(), json)),
        }
    }
}
