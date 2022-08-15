use std::{env, io::Read, os::unix::net::UnixListener};

use anyhow::Context;
use error::AppError;
use tracing::{error, info};

use crate::connection::{Request, RequestParams};

mod api;
mod auth;
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
    let (access_token, refresh_token) = match auth::load_tokens()? {
        Some(tokens) => tokens,
        None => {
            let (access, refresh) = auth::authenticate().await?;
            auth::save_tokens(&access, &refresh)?;
            (access, refresh)
        }
    };
    info!("Tokens retrieved: {}, {}", access_token, refresh_token);

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
    std::fs::remove_file(sock_path)?;
    // TODO: better termination?
    std::process::exit(0);
}
