use crate::{CacheError, EnvError, InvalidJson};
use reqwest::StatusCode;
use serde_json::Value;

const TOKEN_LIMIT: &str = "max_tokens_exceeded";
const BBOX_MODEL_REFUSAL: &str = "Invalid request.";

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AskError {
    #[error("the state {0}")]
    State(InvalidJson),
    #[error("a request must have an API key: set `TYPESAFE_API_KEY`, or call `Client::api_key`")]
    MissingApiKey,
    #[error(
        "the API key must be a valid HTTP header value: \
         remove line breaks and other control characters"
    )]
    InvalidApiKey,
    #[error("the base URL must be an absolute HTTP or HTTPS URL, not `{url}`")]
    InvalidBaseUrl { url: String },
    #[error(
        "the cache has no response for the request, and the client is offline: \
         set `SYSTEM2_CACHE` or call `Client::cache`, and record the response with a run online"
    )]
    Offline,
    #[error("the request has too many tokens{}", token_limit_fix(*long_context))]
    TokenLimit { long_context: bool },
    #[error(
        "the images of the request must have at most 32 MiB, not {size} bytes: \
         split the request, or send smaller images"
    )]
    ByteLimit { size: usize },
    #[error(
        "the request refers to the image `{reference}`, and no `Image` holds it now: \
         keep the `Image` until `ask` returns"
    )]
    MissingImage { reference: String },
    #[error(
        "the model of a Bbox must be `jev-1.14.0` or a later version, not `{model}`: \
         call `Client::model(\"jev-1.14.0\")`"
    )]
    BboxModel { model: String },
    #[error("the response must arrive within the timeout: raise it with `Client::timeout`")]
    Timeout,
    #[error("the connection to the API failed: check the network and the base URL")]
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
    #[error(transparent)]
    Cache(CacheError),
}

impl AskError {
    pub(crate) fn from_status(
        status: StatusCode,
        body: &str,
        request_id: Option<String>,
        long_context: bool,
        bbox_model: Option<&str>,
    ) -> Self {
        let response: Value = serde_json::from_str(body).unwrap_or_default();
        let detail = &response["detail"];
        if detail["error_type"] == TOKEN_LIMIT {
            return AskError::TokenLimit { long_context };
        }
        let message = api_message(detail);

        // the api refuses a bbox on a model before `jev-1.14.0` with only this message
        if let Some(model) = bbox_model
            && status == StatusCode::BAD_REQUEST
            && message.as_deref() == Some(BBOX_MODEL_REFUSAL)
        {
            let model = model.to_owned();
            return AskError::BboxModel { model };
        }
        let reason = status.canonical_reason().filter(|_| body.trim().is_empty());
        let message = message.unwrap_or_else(|| reason.unwrap_or(body).to_owned());
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

    // the same variant as a bad value from a builder method, which `ask` refuses
    pub(crate) fn from_env_error(error: EnvError) -> Self {
        match error {
            EnvError::InvalidApiKey => AskError::InvalidApiKey,
            EnvError::InvalidBaseUrl { url } => AskError::InvalidBaseUrl { url },
            EnvError::Cache(error) => AskError::Cache(error),
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

fn token_limit_fix(long_context: bool) -> &'static str {
    if long_context {
        " for long context: make the state smaller, or split the questions into many requests"
    } else {
        ": raise the limits with `Client::long_context(true)`"
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

            let error = AskError::from_status(status, &body, None, false, None);

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
        let cases = [
            (
                false,
                "the request has too many tokens: \
                 raise the limits with `Client::long_context(true)`",
            ),
            (
                true,
                "the request has too many tokens for long context: \
                 make the state smaller, or split the questions into many requests",
            ),
        ];
        for (long_context, message) in cases {
            let error =
                AskError::from_status(StatusCode::BAD_REQUEST, &body, None, long_context, None);

            assert!(
                matches!(error, AskError::TokenLimit { long_context: found } if found == long_context),
                "{error:?}"
            );
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn from_status_finds_a_bbox_on_an_older_model() {
        let invalid_request =
            json!({ "detail": { "error_type": "api_usage_error", "message": "Invalid request." } })
                .to_string();
        let other_message = json!({ "detail": "Invalid image request." }).to_string();
        let cases = [
            (400, &invalid_request, Some("jev-1.13.0"), true),
            (400, &invalid_request, None, false),
            (400, &other_message, Some("jev-1.13.0"), false),
            (422, &invalid_request, Some("jev-1.13.0"), false),
        ];
        for (code, body, bbox_model, is_bbox_model) in cases {
            let status = StatusCode::from_u16(code).unwrap();

            let error = AskError::from_status(status, body, None, false, bbox_model);

            if is_bbox_model {
                assert!(
                    matches!(&error, AskError::BboxModel { model } if model == "jev-1.13.0"),
                    "{error:?}"
                );
                assert_eq!(
                    error.to_string(),
                    "the model of a Bbox must be `jev-1.14.0` or a later version, \
                     not `jev-1.13.0`: call `Client::model(\"jev-1.14.0\")`"
                );
            } else {
                assert!(
                    matches!(error, AskError::Http { .. }),
                    "{code} {body} {error:?}"
                );
            }
        }
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
