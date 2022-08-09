use reqwest::header::CONTENT_TYPE;
use reqwest::Client;
use serde_json::json;

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
        let user_data: serde_json::Value = serde_json::from_str(&json)?;
        let id = user_data["data"]["id"]
            .as_str()
            .map(String::from)
            .ok_or(AppError::ApiResponseNotFound("id".to_owned()))?;
        Ok(id)
    }

    /// Calls `users/:id/timelines/reverse_chronological` endpoint to fetch the home timeline of the user.
    pub async fn timeline(&self, since_id: Option<&str>) -> Result<Vec<Tweet>, AppError> {
        let endpoint = format!(
            "https://api.twitter.com/2/users/{}/timelines/reverse_chronological",
            self.user_id
        );
        let body = match since_id {
            Some(id) => json!({ "since_id": since_id }).to_string(),
            None => "".to_string(),
        };
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
        let resp: serde_json::Value = serde_json::from_str(&json)?;
        println!("{:?}", resp);
        let data = &resp["data"];
        let tweets: Vec<Tweet> = serde_json::value::from_value(data.clone())?;
        Ok(tweets)
    }
}
