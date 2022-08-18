use std::{
    env,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
};

use anyhow::Context;
use error::AppError;
use tracing::{error, info, warn};

use crate::connection::{Request, Response, ResponseContent, ResponseError, JSONRPC_VERSION};

mod api;
mod auth;
mod connection;
mod error;
mod filter;
mod tweet;

const VERSION: &str = "0.1.0";

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
    let listener = UnixListener::bind(sock_path).map_err(AppError::SocketBind)?;

    let client = api::ApiClient::new(access_token).await?;

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let stream_ = stream.try_clone()?;
                let mut reader = BufReader::new(stream_);
                let mut payload = String::new();
                reader.read_line(&mut payload)?;

                let req: Request =
                    serde_json::from_str(&payload).map_err(AppError::SocketPayloadParse)?;
                let req_ = req.clone();
                let resp = connection::handle_request(req, &client).await;
                match resp {
                    Ok(resp) => {
                        let json =
                            serde_json::to_string(&resp).map_err(AppError::ApiResponseSerialize)?;
                        stream.write_all(json.as_bytes())?;
                        stream.flush()?;
                    }
                    Err(err) => {
                        warn!("something bad happened: {:?}", err);
                        let resp_err: ResponseError = err.into();
                        let resp = Response {
                            jsonrpc: JSONRPC_VERSION.to_string(),
                            content: ResponseContent::Error(resp_err),
                            id: req_.id,
                        };
                        let json =
                            serde_json::to_string(&resp).map_err(AppError::ApiResponseSerialize)?;
                        stream.write_all(json.as_bytes())?;
                        stream.flush()?;
                    }
                }
            }
            Err(_) => continue,
        }
    }

    Ok(())
}

fn fini() -> Result<(), AppError> {
    let sock_path = env::var("SOCKET_PATH")?;
    std::fs::remove_file(sock_path)?;
    // TODO: better termination?
    std::process::exit(0);
}
