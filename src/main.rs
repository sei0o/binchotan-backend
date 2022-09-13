use crate::{auth::Auth, config::Config, connection::Request};
use anyhow::Context;
use connection::Handler;
use credential::CredentialStore;
use error::AppError;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    path::{Path, PathBuf},
};
use thiserror::Error;

mod api;
mod auth;
mod cache;
mod config;
mod connection;
mod credential;
mod error;
mod filter;
mod methods;
mod tweet;

const VERSION: &str = "0.1.0";

#[tokio::main]
async fn main() -> Result<(), AppError> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::new()?;

    let result = start(config).await;
    if let Err(err) = &result {
        println!("{}", err);
    }

    result
}

async fn start(config: Config) -> Result<(), AppError> {
    let auth = Auth::new(
        config.twitter_client_id,
        config.twitter_client_secret,
        config.redirect_host,
        config.scopes.clone(),
    );
    let store = CredentialStore::new(config.cache_path.into(), auth)?;

    let mut listener = Listener::new(&config.socket_path)?;

    let sock_path = config.socket_path.clone();
    ctrlc::set_handler(move || {
        std::fs::remove_file(&sock_path).unwrap();
        std::process::exit(0);
    })
    .context("could not create a Ctrl-C(SIGINT) handler")?;

    // validate filters' scopes in advance
    filter::Filter::load(config.filter_dir.as_ref(), &config.scopes)?;

    let handler = Handler {
        store,
        filter_path: config.filter_dir.clone(),
        scopes: config.scopes.clone(),
    };

    listener.listen(handler).await?;

    Ok(())
}

#[derive(Debug, Error)]
pub enum ListenerError {
    #[error("could not bind to the socket. another backend might be running?")]
    Bind(#[source] std::io::Error),
    #[error("could not parse the socket payload")]
    Parse(#[source] serde_json::Error),
}

struct Listener {
    socket: UnixListener,
    path: PathBuf,
}

impl Listener {
    pub fn new<T: AsRef<Path>>(socket_path: T) -> Result<Self, ListenerError> {
        Ok(Self {
            socket: UnixListener::bind(socket_path.as_ref()).map_err(ListenerError::Bind)?,
            path: socket_path.as_ref().to_owned(),
        })
    }

    pub async fn listen(&mut self, mut handler: Handler) -> Result<(), AppError> {
        for stream in self.socket.incoming() {
            match stream {
                Ok(mut stream) => {
                    let stream_ = stream.try_clone()?;
                    let mut reader = BufReader::new(stream_);
                    let mut payload = String::new();
                    reader.read_line(&mut payload)?;

                    let req: Request =
                        serde_json::from_str(&payload).map_err(ListenerError::Parse)?;
                    let resp = handler.handle(req).await;
                    // SAFETY: Response is serde::Serialize so it should always be able to be serialized
                    let json = serde_json::to_string(&resp).unwrap();

                    stream.write_all(json.as_bytes())?;
                    stream.flush()?;
                }
                Err(_) => continue,
            }
        }

        Ok(())
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        std::fs::remove_file(&self.path).unwrap();
    }
}
