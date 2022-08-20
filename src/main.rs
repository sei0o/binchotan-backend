use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    path::Path,
};

use anyhow::Context;
use error::AppError;
use tracing::{error, info, warn};

use crate::{auth::Auth, config::Config, connection::Request};

mod api;
mod auth;
mod config;
mod connection;
mod error;
mod filter;
mod tweet;

const VERSION: &str = "0.1.0";

#[tokio::main]
async fn main() -> Result<(), AppError> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::new()?;
    let sock_path = config.socket_path.clone();

    ctrlc::set_handler({
        let path = config.socket_path.clone();
        move || match fini(&path) {
            Ok(_) => {}
            Err(err) => error!("{}", err),
        }
    })
    .context("could not create a Ctrl-C(SIGINT) handler")?;

    let result = start(config).await;
    if let Err(err) = &result {
        println!("{}", err);
    }
    fini(&sock_path)?;
    result
}

async fn start(config: Config) -> Result<(), AppError> {
    let auth = Auth::new(
        config.twitter_client_id,
        config.twitter_client_secret,
        config.cache_path,
    );
    let client = auth.client().await?;

    let listener = UnixListener::bind(config.socket_path).map_err(AppError::SocketBind)?;

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let stream_ = stream.try_clone()?;
                let mut reader = BufReader::new(stream_);
                let mut payload = String::new();
                reader.read_line(&mut payload)?;

                let req: Request =
                    serde_json::from_str(&payload).map_err(AppError::SocketPayloadParse)?;
                let resp =
                    connection::handle_request(req, &client, config.filter_dir.clone()).await;

                let json = serde_json::to_string(&resp).map_err(AppError::ApiResponseSerialize)?;
                stream.write_all(json.as_bytes())?;
                stream.flush()?;
            }
            Err(_) => continue,
        }
    }

    Ok(())
}

fn fini<P>(sock_path: P) -> Result<(), AppError>
where
    P: AsRef<Path>,
{
    std::fs::remove_file(sock_path.as_ref())?;
    // TODO: better termination?
    std::process::exit(0);
}
