use reqwest::Client;
use serde_json::json;

// TODO: use a crate dedicated for the twitter api?
// TODO: pagination

pub struct ApiClient {
    client: Client,
    user_id: String,
    access_token: String,
}

impl ApiClient {
    pub fn new(access_token: String) -> Result<Self> {
        let client = Client::new();
        let user_id = Self::id_for_token(&client, &access_token)?;

        Ok(Self {
            client,
            user_id,
            access_token,
        })
    }

    pub async fn id_for_token(client: &Client, access_token: &str) -> Result<String, Error> {
        let endpoint = "https://api.twitter.com/2/users/me";
        let json = client
            .get(endpoint)
            .bearer_auth(access_token.to_owned())
            .send()
            .await?
            .text()
            .await?;
        let user_data: serde_json::Value = serde_json::from_str(&json)?;
        user_data["data"]["id"]
            .as_str()
            .map(String::from)
            .context("could not retrieve the user ID")?;
    }

    /// Calls `users/:id/timelines/reverse_chronological` endpoint to fetch the home timeline of the user.
    pub async fn timeline(&self, since_id: &str) -> Result<Vec<Tweet>, Error> {
        let endpoint = format!(
            "https://api.twitter.com/2/users/{}/timelines/reverse_chronological",
            self.user_id
        );
        let json = self
            .client
            .get(endpoint)
            .body(json!({ "since_id": since_id }).to_owned())
            .bearer_auth(self.access_token.to_owned())
            .send()
            .await?
            .text()
            .await?;
        let user_data: serde_json::Value = serde_json::from_str(&json)?;
        user_data["data"]["id"].as_str().map(String::from)?;
    }
}
