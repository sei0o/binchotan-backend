use anyhow::Context;
use oauth2::{
    basic::BasicClient, reqwest::async_http_client, AuthUrl, AuthorizationCode, ClientId,
    ClientSecret, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, RefreshToken, Scope,
    TokenResponse, TokenUrl,
};
use std::{borrow::Cow, collections::HashSet};
use thiserror::Error;
use tokio::{
    sync::mpsc::{self, error::TryRecvError},
    task::JoinHandle,
};
use tracing::info;
use url::Url;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("could not start the redirect server. The port might be already occupied: {0}")]
    ServerLaunch(Box<dyn std::error::Error + Send + Sync + 'static>),
    #[error("could not receive a request: {0}")]
    ServerListen(std::io::Error),
    #[error("no authorization code was returned")]
    NoAuthorizationCode,
    #[error("no state was returned")]
    NoState,
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error("failed to exchange authorization code for access token: {0:?}")]
    Exchange(#[source] anyhow::Error),
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
    _handle: JoinHandle<()>,
    tx: mpsc::Sender<RedirectServerRequest>,
}

impl Auth {
    pub fn new(
        client_id: String,
        client_secret: String,
        redirect_host: String,
        scopes: HashSet<String>,
    ) -> Self {
        let client = create_client(client_id.clone(), client_secret.clone());
        let (tx, rx) = mpsc::channel(10);
        let handle = start_server(redirect_host.clone(), client, rx);

        Self {
            client_id,
            client_secret,
            redirect_host,
            scopes,
            // the server will stop when Auth is dropped
            _handle: handle,
            tx,
        }
    }

    pub async fn start_auth(
        &self,
        callback: impl FnOnce(String, String) + Send + 'static,
    ) -> Result<String, AuthError> {
        let client = create_client(self.client_id.clone(), self.client_secret.clone())
            .set_redirect_uri(RedirectUrl::new(format!("http://{}", self.redirect_host))?);

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let scopes = self.scopes.clone();
        let (auth_url, state) = client
            .authorize_url(CsrfToken::new_random)
            .add_scopes(scopes.into_iter().map(Scope::new))
            .set_pkce_challenge(pkce_challenge)
            .url();

        // save them for later verification
        info!(
            "sent to redirect server: state = {}, pkce_verifier = {}",
            state.secret(),
            pkce_verifier.secret()
        );
        self.tx
            .send(RedirectServerRequest {
                state,
                pkce_verifier,
                callback: Box::new(callback),
            })
            .await
            .or(Err(anyhow::anyhow!(
                "could not send a request to the redirect server"
            )))?;

        Ok(auth_url.into())
    }

    /// Refresh tokens to obtain a fresh access token using the refresh token received in advance.
    pub async fn refresh_tokens(
        &self,
        refresh_token: String,
    ) -> Result<(String, String), AuthError> {
        let refresh_token = RefreshToken::new(refresh_token);
        let scopes = self.scopes.clone();
        let client = create_client(self.client_id.clone(), self.client_secret.clone())
            .set_redirect_uri(RedirectUrl::new("http://127.0.0.1:31337".to_owned())?);

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
}

fn create_client(client_id: String, client_secret: String) -> BasicClient {
    // SAFETY: it's safe to unwrap here because we are just converting constant strings into dedicated structs.
    BasicClient::new(
        ClientId::new(client_id),
        Some(ClientSecret::new(client_secret)),
        AuthUrl::new("https://twitter.com/i/oauth2/authorize".to_owned()).unwrap(),
        Some(TokenUrl::new("https://api.twitter.com/2/oauth2/token".to_owned()).unwrap()),
    )
}

fn start_server(
    redirect_host: String,
    client: BasicClient,
    rx: mpsc::Receiver<RedirectServerRequest>,
) -> JoinHandle<()> {
    tokio::task::spawn(async {
        let mut server = RedirectServer::new(client, redirect_host, rx);
        server.start().await.unwrap();
    })
}

// Represents (state, pkce_verifier, callback)
pub(crate) struct RedirectServerRequest {
    state: CsrfToken,
    pkce_verifier: PkceCodeVerifier,
    callback: Box<dyn FnOnce(String, String) + Send + 'static>,
}

struct RedirectServer {
    states: Vec<RedirectServerRequest>,
    client: BasicClient,
    redirect_host: String,
    rx: mpsc::Receiver<RedirectServerRequest>,
}

impl RedirectServer {
    fn new(
        client: BasicClient,
        redirect_host: String,
        rx: mpsc::Receiver<RedirectServerRequest>,
    ) -> Self {
        Self {
            states: vec![],
            client,
            redirect_host,
            rx,
        }
    }

    async fn start(&mut self) -> Result<(), AuthError> {
        // TODO: use async http server implementation (e.g. tide)
        let server =
            tiny_http::Server::http(self.redirect_host.clone()).map_err(AuthError::ServerLaunch)?;
        loop {
            if let Some(req) = server.try_recv().map_err(AuthError::ServerListen)? {
                match self.handle_request(req).await {
                    Ok(_) => {}
                    Err(err) => {
                        tracing::error!("could not authenticate: {:?}", err)
                    }
                }
            }

            match self.rx.try_recv() {
                Ok(req) => self.states.push(req),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    // shutdown
                    info!("shutting down the redirect server...");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_request(&mut self, req: tiny_http::Request) -> Result<(), AuthError> {
        let pairs = Url::parse(&format!("http://{}/{}", self.redirect_host, req.url()))?;
        let code = pairs
            .query_pairs()
            .find_map(|(k, v)| match k {
                Cow::Borrowed("code") => Some(AuthorizationCode::new(v.to_string())),
                _ => None,
            })
            .ok_or(AuthError::NoAuthorizationCode)?;
        let state = pairs
            .query_pairs()
            .find_map(|(k, v)| match k {
                Cow::Borrowed("state") => Some(CsrfToken::new(v.to_string())),
                _ => None,
            })
            .ok_or(AuthError::NoState)?;

        let (acc, refr, callback) = self.generate_tokens(code, state).await?;

        info!("got tokens : {},  {}", acc, refr);
        callback(acc, refr);

        // return 200 OK
        let resp = tiny_http::Response::from_string(
        "Authentication succeeded! Now you can safely close this page and go back to your frontend.",);
        req.respond(resp)?;

        Ok(())
    }

    /// Ask the authorization server to exchange the authorization code for access/refresh token.
    async fn generate_tokens(
        &mut self,
        code: AuthorizationCode,
        state: CsrfToken,
    ) -> Result<
        (
            String,
            String,
            Box<dyn FnOnce(String, String) + Send + 'static>,
        ),
        AuthError,
    > {
        // look for the same state
        let idx = self
            .states
            .iter()
            .enumerate()
            .find(|(_i, s)| *(**s).state.secret() == *state.secret())
            .map(|(i, _s)| i)
            .ok_or_else(|| AuthError::InvalidState(state.secret().into()))?;

        let RedirectServerRequest {
            state,
            pkce_verifier,
            callback,
        } = self.states.swap_remove(idx);

        // なんかを忘れている・・・・code か pkce_verifierが誤り
        info!(
            "retrieved: state = {}, pkce_verifier = {}, code = {}\n",
            state.secret(),
            pkce_verifier.secret(),
            code.secret(),
        );
        info!(
            "pkce_challenge should be: {}",
            PkceCodeChallenge::from_code_verifier_sha256(&pkce_verifier).as_str()
        );

        let req = self
            .client
            .exchange_code(code)
            .set_pkce_verifier(pkce_verifier)
            // It seems Twitter requires redirect_uri again on Authorization Code Request.
            // see also: https://www.oauth.com/oauth2-servers/access-tokens/authorization-code-request/
            .set_redirect_uri(Cow::Owned(RedirectUrl::new(
                // TODO: remove hard-coded redirect url
                "http://127.0.0.1:31337".to_owned(),
            )?));
        info!("request: {:?}", req);
        let result = req.request_async(async_http_client).await.map_err(|err| {
            tracing::error!("{:?}", err);
            AuthError::Exchange(err.into())
        })?;
        let access_token = result.access_token().secret().to_owned();
        let refresh_token = match result.refresh_token() {
            Some(x) => x.secret(),
            None => "",
        }
        .to_owned();

        Ok((access_token, refresh_token, callback))
    }
}
