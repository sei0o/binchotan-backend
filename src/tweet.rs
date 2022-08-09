use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Tweet {
    pub id: String,
    pub text: String,
}
