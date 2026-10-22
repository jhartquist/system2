use reqwest::StatusCode;

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
    #[error("the request has too many tokens: use `Client::long_context(true)`")]
    TokenLimit,
    #[error("the response must arrive within the timeout: raise it with `Client::timeout`")]
    Timeout,
    #[error("the connection to the API failed: {0}")]
    Transport(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error(
        "the API refused the request with status {status}: {message}{}",
        format_request_id(request_id.as_deref())
    )]
    Http {
        status: u16,
        message: String,
        request_id: Option<String>,
    },
    #[error("the response {message}{}", format_request_id(request_id.as_deref()))]
    InvalidResponse {
        message: String,
        request_id: Option<String>,
    },
}

impl AskError {
    pub(crate) fn from_status(status: StatusCode, body: &str, request_id: Option<String>) -> Self {
        let _ = (status, body, request_id);
        todo!()
    }

    pub(crate) fn from_reqwest(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            AskError::Timeout
        } else {
            AskError::Transport(Box::new(error))
        }
    }
}

fn format_request_id(id: Option<&str>) -> String {
    let _ = id;
    todo!()
}
