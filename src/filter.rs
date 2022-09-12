use serde::Deserialize;
use std::{
    collections::HashSet,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use thiserror::Error;
use tracing::error;

use crate::{error::AppError, tweet::Tweet};
use mlua::prelude::*;

#[derive(Debug)]
pub struct Filter {
    pub src: String,
    pub meta: FilterMeta,
}

#[derive(Debug, Deserialize)]
pub struct FilterMeta {
    name: String,
    description: String,
    author: String,
    entrypoint: String,
    scopes: HashSet<String>,
}

#[derive(Debug, Error)]
pub enum FilterError {
    #[error("the given path ({0}) is not a directory")]
    PathNotDir(PathBuf),
    #[error("could not parse binchotan.toml")]
    MetaParse(toml::de::Error),
    #[error("Filter `{0}` requires an additional API scopes (permissions): {}. Review the filter and add scopes in your config if you want to.", .1.join(","))]
    InsufficientScopes(String, Vec<String>),
    #[error("other IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl Filter {
    pub fn load(
        dir: &Path,
        available_scopes: &HashSet<String>,
    ) -> Result<Vec<Filter>, FilterError> {
        if !dir.is_dir() {
            return Err(FilterError::PathNotDir(dir.to_owned()));
        }

        dir.read_dir()?
            .filter_map(|entry| match entry {
                Ok(entry) => Some(entry.path()),
                _ => None,
            })
            .filter(|path| path.is_dir())
            .map(|dir| match Self::load_single(&dir, available_scopes) {
                Ok(filter) => Ok(filter),
                Err(err) => {
                    error!("could not load filter in {}/ : {}", dir.display(), err);
                    Err(err)
                }
            })
            .collect()
    }

    fn load_single(dir: &Path, available_scopes: &HashSet<String>) -> Result<Filter, FilterError> {
        if !dir.is_dir() {
            return Err(FilterError::PathNotDir(dir.to_owned()));
        }

        let meta_path = dir.join("binchotan.toml");
        let mut meta_buf = String::new();
        File::open(&meta_path)?.read_to_string(&mut meta_buf)?;
        let meta: FilterMeta = toml::from_str(&meta_buf).map_err(FilterError::MetaParse)?;

        let mut src = String::new();
        File::open(&dir.join(&meta.entrypoint))?.read_to_string(&mut src)?;

        let diff: Vec<String> = meta.scopes.difference(available_scopes).cloned().collect();
        if !diff.is_empty() {
            return Err(FilterError::InsufficientScopes(meta.name, diff));
        }

        Ok(Filter { src, meta })
    }

    /// Applies the filter on the given post. The filter is a Lua script which returns a Tweet or null.
    pub fn run(&self, tweet: &Tweet) -> Result<Option<Tweet>, AppError> {
        let lua = Lua::new();
        lua.globals().set("post", lua.to_value(tweet)?)?;
        let ret = lua.load(&self.src).eval()?;
        let v: Option<Tweet> = lua.from_value(ret)?;
        Ok(v)
    }
}
