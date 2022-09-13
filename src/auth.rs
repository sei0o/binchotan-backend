use anyhow::Context;
use oauth2::{
    basic::BasicClient, reqwest::async_http_client, AuthUrl, AuthorizationCode, ClientId,
    ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl, RefreshToken, Scope, TokenResponse,
    TokenUrl,
};
use std::{borrow::Cow, collections::HashSet};
use thiserror::Error;
use tracing::info;
use url::Url;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("could not start the redirect server. The port might be already occupied: {0}")]
    ServerLaunch(String),
    #[error("no authorization code was returned")]
    NoAuthorizationCode,
    #[error("no state was returned")]
    NoState,
    #[error("invalid state: expected {0}, received {1}")]
    InvalidState(String, String),
    #[error("failed to exchange authorization code for access token")]
    Exchange,
    #[error(transparent)]
    Parse(#[from] url::ParseError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub struct Auth {
    client_id: String,
    client_secret: String,
    redirect_host: String,
    pub scopes: HashSet<String>,
}

impl Auth {
    pub fn new(
        client_id: String,
        client_secret: String,
        redirect_host: String,
        scopes: HashSet<String>,
    ) -> Self {
        Self {
            client_id,
            client_secret,
            redirect_host,
            scopes,
        }
    }

    /// Authenticate to Twitter.
    pub async fn generate_tokens(&self) -> Result<(String, String), AuthError> {
        let client = self
            .create_client()
            .set_redirect_uri(RedirectUrl::new(format!("http://{}", self.redirect_host))?);

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let scopes = self.scopes.clone();
        let (auth_url, state) = client
            .authorize_url(CsrfToken::new_random)
            .add_scopes(scopes.into_iter().map(Scope::new))
            .set_pkce_challenge(pkce_challenge)
            .url();

        // use a web server
        open::that(auth_url.as_str()).unwrap_or_else(|_| println!("Browse to: {}", auth_url));
        let server = tiny_http::Server::http(self.redirect_host.clone())
            .map_err(|e| AuthError::ServerLaunch(e.to_string()))?;
        let req = server.recv()?;
        let pairs = Url::parse(&format!("http://{}/{}", self.redirect_host, req.url()))?;
        let auth_code = pairs
            .query_pairs()
            .find_map(|(k, v)| match k {
                Cow::Borrowed("code") => Some(v.to_string()),
                _ => None,
            })
            .ok_or(AuthError::NoAuthorizationCode)?;
        let state_returned = pairs
            .query_pairs()
            .find_map(|(k, v)| match k {
                Cow::Borrowed("state") => Some(v.to_string()),
                _ => None,
            })
            .ok_or(AuthError::NoState)?;
        if state.secret() != &state_returned {
            return Err(AuthError::InvalidState(
                state.secret().into(),
                state_returned,
            ));
        }

        let result = client
            .exchange_code(AuthorizationCode::new(auth_code))
            .set_pkce_verifier(pkce_verifier)
            .request_async(async_http_client)
            .await
            .map_err(|_| AuthError::Exchange)?;
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
    ) -> Result<(String, String), AuthError> {
        let refresh_token = RefreshToken::new(refresh_token);
        let scopes = self.scopes.clone();
        let client = self
            .create_client()
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

    fn create_client(&self) -> BasicClient {
        // SAFETY: it's safe to unwrap here because we are just converting constant strings into dedicated structs.
        BasicClient::new(
            ClientId::new(self.client_id.clone()),
            Some(ClientSecret::new(self.client_secret.clone())),
            AuthUrl::new("https://twitter.com/i/oauth2/authorize".to_owned()).unwrap(),
            Some(TokenUrl::new("https://api.twitter.com/2/oauth2/token".to_owned()).unwrap()),
        )
    }
}
