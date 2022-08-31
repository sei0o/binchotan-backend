use crate::{
    api::ApiClient,
    cache::{Cache, TokenPair},
    error::AppError,
};
use anyhow::{anyhow, Context};
use oauth2::{
    basic::BasicClient, reqwest::async_http_client, AuthUrl, AuthorizationCode, ClientId,
    ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl, RefreshToken, Scope, TokenResponse,
    TokenUrl,
};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};
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

    pub fn user_ids(&self) -> Result<Vec<String>, AppError> {
        let cache = Cache::new(self.cache_path.clone(), self.scopes.clone())?;
        Ok(cache.content.accounts.keys().cloned().collect())
    }

    pub async fn client_for(&self, user_id: &str) -> Result<ApiClient, AppError> {
        let mut cache = Cache::new(self.cache_path.clone(), self.scopes.clone())?;

        // invalidate if the scopes are updated
        if cache.content.scopes != self.scopes {
            return Err(AppError::TokenExpired(Some(user_id.to_owned())));
        }

        let token = cache
            .tokens_for(user_id)
            .ok_or_else(|| AppError::RpcUnknownAccount(user_id.into()))?;

        if !ApiClient::validate_token(&token.access_token).await? {
            return match self.refresh_tokens(token.refresh_token.clone()).await {
                Ok((acc, refr)) => {
                    let pair = cache.content.accounts.get_mut(user_id).unwrap();
                    *pair = TokenPair {
                        access_token: acc.clone(),
                        refresh_token: refr,
                    };
                    cache.save()?;
                    ApiClient::new(acc).await
                }
                Err(_) => return Err(AppError::TokenExpired(Some(user_id.to_owned()))),
            };
        }

        ApiClient::new(token.access_token.clone()).await
    }

    pub async fn clients(&self) -> Result<HashMap<String, ApiClient>, AppError> {
        let mut cache = Cache::new(self.cache_path.clone(), self.scopes.clone())?;

        // invalidate if the scopes are updated
        if cache.content.scopes != self.scopes {
            cache.content.accounts = HashMap::new();
            cache.save()?;
            return Ok(HashMap::new());
        }

        let mut res = HashMap::new();
        for (id, acc) in cache.content.accounts.iter_mut() {
            if ApiClient::validate_token(&acc.access_token).await? {
                let client = ApiClient::new(acc.access_token.clone()).await?;
                res.insert(id.clone(), client);
            };
        }
        // TODO: update cache

        Ok(res)
    }

    pub async fn add_user(&self) -> Result<String, AppError> {
        let (acc, refr) = self.generate_tokens().await?;
        let client = ApiClient::new(acc.clone()).await?;

        // update cache
        let mut cache = Cache::new(self.cache_path.clone(), self.scopes.clone())?;
        let pair = TokenPair {
            access_token: acc,
            refresh_token: refr,
        };
        cache.add_tokens(client.user_id.clone(), pair);
        cache.save()?;

        Ok(client.user_id)
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

    /// Refresh tokens to obtain a fresh access token using the refresh token received in advance.
    pub async fn refresh_tokens(
        &self,
        refresh_token: String,
    ) -> Result<(String, String), AppError> {
        let refresh_token = RefreshToken::new(refresh_token);
        let scopes = self.scopes.clone();
        let client = self
            .create_client()?
            .set_redirect_uri(RedirectUrl::new("http://localhost:31337".to_owned())?);

        let result = client
            .exchange_refresh_token(&refresh_token)
            .add_scopes(scopes.into_iter().map(Scope::new))
            .request_async(async_http_client)
            .await
            .context("failed to exchange refresh token for access token")?;
        let access_token = result.access_token().secret().to_owned();
        let new_refresh_token = match result.refresh_token() {
            Some(x) => x.secret(),
            None => refresh_token.secret(),
        }
        .to_owned();

        info!("Tokens refreshed: {}, {}", access_token, new_refresh_token);

        Ok((access_token, new_refresh_token))
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
}
