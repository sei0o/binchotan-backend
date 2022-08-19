use std::collections::HashMap;

use reqwest::header::CONTENT_TYPE;
use reqwest::Client;
use tracing::debug;

use crate::connection::HttpMethod;
use crate::error::AppError;
use crate::tweet::Tweet;

// TODO: use a crate dedicated for the twitter api?
// TODO: pagination

pub struct ApiClient {
    client: Client,
    user_id: String,
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

    pub async fn id_for_token(client: &Client, access_token: &str) -> Result<String, AppError> {
        let endpoint = "https://api.twitter.com/2/users/me";
        let json = client
            .get(endpoint)
            .bearer_auth(access_token.to_owned())
            .send()
            .await?
            .text()
            .await?;
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
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<Vec<Tweet>, AppError> {
        let endpoint = format!(
            "https://api.twitter.com/2/users/{}/timelines/reverse_chronological",
            self.user_id
        );
        let body = serde_json::to_string(params).map_err(AppError::RpcParamsParse)?;
        let json = self
            .client
            .get(endpoint)
            .body(body)
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
