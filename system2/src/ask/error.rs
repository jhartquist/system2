use crate::InvalidJson;
use reqwest::StatusCode;
use serde_json::Value;

const TOKEN_LIMIT: &str = "max_tokens_exceeded";

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
        let response: Value = serde_json::from_str(body).unwrap_or_default();
        let detail = &response["detail"];
        if detail["error_type"] == TOKEN_LIMIT {
            return AskError::TokenLimit;
        }
        let reason = status.canonical_reason().filter(|_| body.trim().is_empty());
        let message = api_message(detail).unwrap_or_else(|| reason.unwrap_or(body).to_owned());
        AskError::Http {
            status: status.as_u16(),
            message,
            request_id,
        }
    }

    pub(crate) fn from_reqwest(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            AskError::Timeout
        } else {
            AskError::Transport(Box::new(error))
        }
    }
}

fn api_message(detail: &Value) -> Option<String> {
    match detail {
        Value::String(message) => Some(message.clone()),
        Value::Array(entries) => {
            let messages: Vec<&str> = entries
                .iter()
                .filter_map(|entry| entry["msg"].as_str())
                .collect();
            (!messages.is_empty()).then(|| messages.join("; "))
        }
        _ => detail["message"].as_str().map(str::to_owned),
    }
}

fn format_request_id(id: Option<&str>) -> String {
    id.map(|id| format!(" (request ID `{id}`)"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{AskError, format_request_id};
    use reqwest::StatusCode;
    use serde_json::json;

    #[test]
    fn from_status_reads_the_message_of_the_api() {
        let unknown_model = json!({
            "detail": { "error_type": "api_usage_error", "message": "Unknown model: jev-1.14" },
        });
        let too_many = json!({ "detail": "Too many choices. Must have at most 255 choices." });
        let wrong_types = json!({
            "detail": [
                { "type": "string_type", "msg": "Input should be a valid string" },
                { "type": "dict_type", "msg": "Input should be a valid dictionary" },
            ],
        });
        let cases = [
            (400, unknown_model.to_string(), "Unknown model: jev-1.14"),
            (
                400,
                too_many.to_string(),
                "Too many choices. Must have at most 255 choices.",
            ),
            (
                422,
                wrong_types.to_string(),
                "Input should be a valid string; Input should be a valid dictionary",
            ),
            (503, r#"{"error":"busy"}"#.to_owned(), r#"{"error":"busy"}"#),
            (
                502,
                "<html>Bad Gateway</html>".to_owned(),
                "<html>Bad Gateway</html>",
            ),
            (502, String::new(), "Bad Gateway"),
            (401, " \n".to_owned(), "Unauthorized"),
        ];
        for (code, body, expected) in cases {
            let status = StatusCode::from_u16(code).unwrap();

            let error = AskError::from_status(status, &body, None);

            assert_eq!(
                error.to_string(),
                format!("the API refused the request with status {code}: {expected}"),
                "{body:?}"
            );
        }
    }

    #[test]
    fn from_status_finds_the_token_limit() {
        let body = json!({ "detail": { "error_type": "max_tokens_exceeded" } }).to_string();

        let error = AskError::from_status(StatusCode::BAD_REQUEST, &body, None);

        assert!(matches!(error, AskError::TokenLimit));
        assert_eq!(
            error.to_string(),
            "the request has too many tokens: use `Client::long_context(true)`"
        );
    }

    #[test]
    fn format_request_id_adds_the_id_if_present() {
        assert_eq!(
            format_request_id(Some("req_01a13d1fbca4728f8f7c1cdd312bd3a5")),
            " (request ID `req_01a13d1fbca4728f8f7c1cdd312bd3a5`)"
        );
        assert_eq!(format_request_id(None), "");
    }
}
