use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(transparent)]
pub struct Tweet(serde_json::Value);
