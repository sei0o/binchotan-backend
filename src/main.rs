use std::{
    borrow::Cow,
    collections::HashMap,
    env,
    fs::{self, File},
    io::{Read, Write},
    os::unix::net::UnixListener,
};

use anyhow::{anyhow, Context};
use error::AppError;
use oauth2::{
    basic::BasicClient, reqwest::async_http_client, AuthUrl, AuthorizationCode, ClientId,
    ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use serde_json::json;
use tracing::{error, info};
use url::Url;

use crate::connection::{Request, RequestParams};

mod api;
mod connection;
mod error;
mod tweet;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    ctrlc::set_handler(move || match fini() {
        Ok(_) => {}
        Err(err) => error!("{}", err),
    })
    .context("could not create a Ctrl-C(SIGINT) handler")?;

    let result = start().await;
    if let Err(err) = &result {
        println!("{}", err);
    }
    fini()?;
    result
}

async fn start() -> Result<(), AppError> {
    let (access_token, refresh_token) = match load_tokens()? {
        Some(tokens) => tokens,
        None => {
            let (access, refresh) = authenticate().await?;
            save_tokens(&access, &refresh)?;
            (access, refresh)
        }
    };

    let sock_path = env::var("SOCKET_PATH")?;
    let listener = UnixListener::bind(sock_path)?;

    let client = api::ApiClient::new(access_token).await?;

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let mut payload = String::new();
                stream.read_to_string(&mut payload)?;
                let req: Request =
                    serde_json::from_str(&payload).map_err(AppError::SocketPayloadParse)?;
                info!("{:?}", req);
                req.validate()?;

                match req.params {
                    RequestParams::Plain {
                        http_method,
                        endpoint,
                        api_params,
                    } => {
                        todo!()
                    }
                    RequestParams::HomeTimeline(api_params) => {
                        let tweets = client.timeline(&api_params).await?;
                        info!("{:?}", tweets);
                    }
                }
            }
            Err(err) => continue,
        }
    }

    // TODO: call filters in Lua
    // TODO: return filtered tweets over the socket
    // TODO: mock frontend
    // TODO: socket protocol?

    Ok(())
}

fn fini() -> Result<(), AppError> {
    let sock_path = env::var("SOCKET_PATH")?;
    fs::remove_file(sock_path)?;
    // TODO: better termination?
    std::process::exit(0);
}

// Autenticate to Twitter.
// TODO: cache access token / refresh tokens locally?
async fn authenticate() -> Result<(String, String), AppError> {
    let client_id = env::var("TWITTER_CLIENT_ID")?;
    let client_secret = env::var("TWITTER_CLIENT_SECRET")?;

    let client = create_client(client_id, client_secret)?
        .set_redirect_uri(RedirectUrl::new("https://localhost".to_owned())?);

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, state) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("users.read".to_owned()))
        .add_scope(Scope::new("tweet.read".to_owned()))
        .add_scope(Scope::new("offline.access".to_owned()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    println!("Browse to: {}", auth_url);
    // TODO: use a web server instead?
    println!("Paste the URL where you were redirected: ");

    let mut redirected_url = String::new();
    let stdin = std::io::stdin();
    stdin
        .read_line(&mut redirected_url)
        .context("could not read STDIN")?;
    let pairs = Url::parse(&redirected_url)?;
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

    Ok((access_token, refresh_token))
}

fn create_client(id: String, secret: String) -> Result<BasicClient, AppError> {
    Ok(BasicClient::new(
        ClientId::new(id),
        Some(ClientSecret::new(secret)),
        AuthUrl::new("https://twitter.com/i/oauth2/authorize".to_owned())
            .map_err(|x| AppError::OAuth(x.into()))?,
        Some(
            TokenUrl::new("https://api.twitter.com/2/oauth2/token".to_owned())
                .map_err(|x| AppError::OAuth(x.into()))?,
        ),
    ))
}

fn save_tokens(access_token: &str, refresh_token: &str) -> Result<(), AppError> {
    let cache_path = env::var("CACHE_PATH")?;
    let mut file = File::create(cache_path)?;
    file.write_all(
        json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
        })
        .to_string()
        .as_bytes(),
    )?;

    Ok(())
}

fn load_tokens() -> Result<Option<(String, String)>, AppError> {
    let cache_path = env::var("CACHE_PATH")?;
    let mut file = match File::open(cache_path) {
        Ok(file) => file,
        Err(x) if x.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(x) => return Err(x).map_err(AppError::Io),
    };
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    let json: HashMap<&str, serde_json::Value> =
        serde_json::from_str(&content).map_err(AppError::CacheParse)?;
    Ok(Some((
        json["access_token"].to_string(),
        json["refresh_token"].to_string(),
    )))
}
