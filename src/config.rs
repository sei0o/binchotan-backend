use std::{env, path::PathBuf};

use crate::error::AppError;

pub struct Config {
    pub twitter_client_id: String,
    pub twitter_client_secret: String,
    pub socket_path: String,
    pub cache_path: String,
    pub filter_dir: PathBuf,
}

impl Config {
    pub fn new() -> Result<Self, AppError> {
        Ok(Self {
            twitter_client_id: env::var("TWITTER_CLIENT_ID")?,
            twitter_client_secret: env::var("TWITTER_CLIENT_SECRET")?,
            socket_path: env::var("SOCKET_PATH")?,
            cache_path: env::var("CACHE_PATH")?,
            filter_dir: env::var("FILTER_DIR")?.into(),
        })
    }
}
