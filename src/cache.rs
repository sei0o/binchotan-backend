use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Deserialize, Serialize, Default)]
pub struct TokenCache {
    pub accounts: Vec<TokenCacheAccount>,
    pub scopes: HashSet<String>,
}

#[derive(Deserialize, Serialize)]
pub struct TokenCacheAccount {
    pub id: String,
    pub access_token: String,
    pub refresh_token: String,
}

pub struct Cache {
    cache_path: PathBuf,
    pub content: TokenCache,
}

impl Cache {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, AppError> {
        let mut file = match File::open(path.as_ref()) {
            Ok(file) => file,
            Err(x) if x.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    cache_path: path.as_ref().to_owned(),
                    content: TokenCache::default(),
                })
            }
            Err(x) => return Err(x).map_err(AppError::Io),
        };
        let mut s = String::new();
        file.read_to_string(&mut s)?;
        let content: TokenCache = serde_json::from_str(&s).map_err(AppError::CacheParse)?;

        Ok(Self {
            cache_path: path.as_ref().to_owned(),
            content,
        })
    }

    pub fn add_tokens(&mut self, acc: TokenCacheAccount) {
        self.content.accounts.push(acc)
    }

    pub fn save(&self) -> Result<(), AppError> {
        let mut file = File::create(&self.cache_path)?;
        file.write_all(serde_json::to_string(&self.content).unwrap().as_bytes())?;
        Ok(())
    }
}
