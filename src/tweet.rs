use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Tweet {
    pub id: String,
    pub text: String,
}
