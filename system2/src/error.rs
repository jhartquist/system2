#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidJson {
    #[error("must serialize to JSON: {0}")]
    Serialize(serde_json::Error),
    #[error("must be text, an object, or an array")]
    Shape,
    #[error("must not be empty")]
    Empty,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AskError {
    #[error("the state {0}")]
    State(InvalidJson),
    #[error("a request must have an API key: set `TYPESAFE_API_KEY`, or call `Client::api_key`")]
    MissingApiKey,
    #[error("the base URL must be an absolute HTTP or HTTPS URL, not `{url}`")]
    InvalidBaseUrl { url: String },
    #[error("the response must arrive within the timeout: raise it with `Client::timeout`")]
    Timeout,
    #[error("the connection to the API failed: {0}")]
    Transport(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("the API refused the request with status {status}: {message}")]
    Http { status: u16, message: String },
    #[error("the response {message}")]
    InvalidResponse { message: String },
}

impl AskError {
    pub(crate) fn from_reqwest(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            AskError::Timeout
        } else {
            AskError::Transport(Box::new(error))
        }
    }
}
