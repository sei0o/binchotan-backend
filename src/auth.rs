use std::{
    borrow::Cow,
    collections::HashSet,
    fs::File,
    io::{Read, Write},
};

use crate::{api::ApiClient, error::AppError};
use anyhow::{anyhow, Context};
use oauth2::{
    basic::BasicClient, reqwest::async_http_client, AuthUrl, AuthorizationCode, ClientId,
    ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};
use tracing::info;
use url::Url;

pub struct Auth {
    client_id: String,
    client_secret: String,
    scopes: HashSet<String>,
    cache_path: String,
}

impl Auth {
    pub fn new(
        client_id: String,
        client_secret: String,
        scopes: HashSet<String>,
        cache_path: String,
    ) -> Self {
        Self {
            client_id,
            client_secret,
            scopes,
            cache_path,
        }
    }

    pub async fn client(&self) -> Result<ApiClient, AppError> {
        let (access_token, refresh_token) = match self.load_tokens().await? {
            Some(tokens) => tokens,
            None => {
                let (access, refresh) = self.generate_tokens().await?;
                self.save_tokens(&access, &refresh)?;
                (access, refresh)
            }
        };

        ApiClient::new(access_token).await
    }

    /// Authenticate to Twitter.
    pub async fn generate_tokens(&self) -> Result<(String, String), AppError> {
        let client = self
            .create_client()?
            .set_redirect_uri(RedirectUrl::new("http://localhost:31337".to_owned())?);

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let scopes = self.scopes.clone();
        let (auth_url, state) = client
            .authorize_url(CsrfToken::new_random)
            .add_scopes(scopes.into_iter().map(Scope::new))
            .set_pkce_challenge(pkce_challenge)
            .url();

        // use a web server
        open::that(auth_url.as_str()).unwrap_or_else(|_| println!("Browse to: {}", auth_url));
        // TODO: let users choose which port to use
        let server = tiny_http::Server::http("localhost:31337")
            .map_err(|e| AppError::ServerLaunch(e.to_string()))?;
        let req = server.recv()?;
        let pairs = Url::parse(&format!("http://localhost:31337/{}", req.url()))?;
        let auth_code = pairs
            .query_pairs()
            .find_map(|(k, v)| match k {
                Cow::Borrowed("code") => Some(v),
                _ => None,
            })
            .context("no authorization code was returned")?
            .to_string();
        let state_returned = pairs
            .query_pairs()
            .find_map(|(k, v)| match k {
                Cow::Borrowed("state") => Some(v.to_string()),
                _ => None,
            })
            .context("no state was returned")?;
        if state.secret() != &state_returned {
            return Err(AppError::OAuth(anyhow!("invalid csrf state")));
        }

        let result = client
            .exchange_code(AuthorizationCode::new(auth_code))
            .set_pkce_verifier(pkce_verifier)
            .request_async(async_http_client)
            .await
            .context("failed to exchange authorization code for access token")?;
        let access_token = result.access_token().secret().to_owned();
        let refresh_token = match result.refresh_token() {
            Some(x) => x.secret(),
            None => "",
        }
        .to_owned();

        info!("Tokens retrieved: {}, {}", access_token, refresh_token);

        // return 200 OK
        let resp = tiny_http::Response::from_string(
            "Authentication succeeded! Now you can safely close this page.",
        );
        req.respond(resp)?;

        Ok((access_token, refresh_token))
    }

    fn create_client(&self) -> Result<BasicClient, AppError> {
        // SAFETY: it's safe to unwrap here because we are just converting constant strings into dedicated structs.
        Ok(BasicClient::new(
            ClientId::new(self.client_id.clone()),
            Some(ClientSecret::new(self.client_secret.clone())),
            AuthUrl::new("https://twitter.com/i/oauth2/authorize".to_owned()).unwrap(),
            Some(TokenUrl::new("https://api.twitter.com/2/oauth2/token".to_owned()).unwrap()),
        ))
    }

    fn save_tokens(&self, access_token: &str, refresh_token: &str) -> Result<(), AppError> {
        let mut file = File::create(&self.cache_path)?;
        let cache = TokenCache {
            access_token: access_token.to_owned(),
            refresh_token: refresh_token.to_owned(),
            scopes: self.scopes.clone(),
        };
        file.write_all(serde_json::to_string(&cache).unwrap().as_bytes())?;

        Ok(())
    }

    pub async fn load_tokens(&self) -> Result<Option<(String, String)>, AppError> {
        match self.load_tokens_from_cache()? {
            Some((access, refresh)) => {
                if ApiClient::validate_token(&access).await? {
                    Ok(Some((access, refresh)))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    fn load_tokens_from_cache(&self) -> Result<Option<(String, String)>, AppError> {
        let mut file = match File::open(&self.cache_path) {
            Ok(file) => file,
            Err(x) if x.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(x) => return Err(x).map_err(AppError::Io),
        };
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let cache: TokenCache = serde_json::from_str(&content).map_err(AppError::CacheParse)?;

        // request authorization again if the scopes are updated
        if cache.scopes != self.scopes {
            return Ok(None);
        }

        Ok(Some((cache.access_token, cache.refresh_token)))
    }
}

#[derive(Deserialize, Serialize)]
pub struct TokenCache {
    access_token: String,
    refresh_token: String,
    scopes: HashSet<String>,
}
