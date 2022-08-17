use crate::{error::AppError, tweet::Tweet};
use mlua::prelude::*;

pub struct Filter {
    pub src: String,
}

impl Filter {
    /// Runs the filter on the given post. The filter is a Lua script which returns a Tweet or null.
    pub fn run(&self, tweet: &Tweet) -> Result<Option<Tweet>, AppError> {
        let lua = Lua::new();
        lua.globals().set("post", lua.to_value(tweet)?)?;
        let ret = lua.load(&self.src).eval()?;
        let v: Option<Tweet> = lua.from_value(ret)?;
        Ok(v)
    }
}
