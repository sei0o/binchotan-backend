use serde::Deserialize;

// We define an enum for HTTP request method since http::Method does not implement serde::Deserialize
#[derive(Debug, Clone, Copy, Deserialize)]
pub enum HttpMethod {
    #[serde(rename = "GET")]
    Get,
    #[serde(rename = "POST")]
    Post,
    #[serde(rename = "PUT")]
    Put,
    #[serde(rename = "DELETE")]
    Delete,
    // Twitter API does not utilize other methods
}

impl From<HttpMethod> for reqwest::Method {
    fn from(h: HttpMethod) -> Self {
        match h {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Delete => reqwest::Method::DELETE,
        }
    }
}
