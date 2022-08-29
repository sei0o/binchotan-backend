use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;

use crate::error::AppError;

#[derive(Deserialize)]
pub struct Config {
    pub twitter_client_id: String,
    pub twitter_client_secret: String,
    pub socket_path: String,
    pub cache_path: String,
    pub filter_dir: PathBuf,
    pub scopes: HashSet<String>,
}

impl Config {
    pub fn new() -> Result<Self, AppError> {
        let config: Config = config::Config::builder()
            .add_source(config::File::with_name("config.toml"))
            .add_source(config::Environment::with_prefix("BINCHOTAN"))
            .build()?
            .try_deserialize()?;

        Ok(config)
    }
}
