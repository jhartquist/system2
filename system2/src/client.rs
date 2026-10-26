use crate::ask::wire::{RawAnswerSet, RawQuestionSet};
use crate::{AskError, AskFuture, QuestionSet, json};
use reqwest::Url;
use reqwest::header::{CONTENT_ENCODING, CONTENT_TYPE, HeaderMap};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use std::{env, fmt};

const BASE_URL: &str = "https://api.typesafe.ai";
const MODEL: &str = "jev-latest";
const TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_ID: &str = "x-typesafe-request-id";

static GLOBAL: Mutex<Option<Client>> = Mutex::new(None);

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    api_key: Option<String>,
    base_url: String,
    model: String,
    timeout: Duration,
}

impl Client {
    #[must_use]
    pub fn new() -> Self {
        Client {
            http: reqwest::Client::new(),
            api_key: None,
            base_url: BASE_URL.to_owned(),
            model: MODEL.to_owned(),
            timeout: TIMEOUT,
        }
    }

    #[must_use]
    pub fn from_env() -> Self {
        let api_key = var("TYPESAFE_API_KEY");
        let base_url = var("TYPESAFE_BASE_URL").unwrap_or_else(|| BASE_URL.to_owned());
        let model = var("TYPESAFE_DEFAULT_MODEL").unwrap_or_else(|| MODEL.to_owned());
        Client {
            api_key,
            base_url,
            model,
            ..Client::new()
        }
    }

    #[must_use]
    pub fn api_key(self, api_key: impl Into<String>) -> Self {
        let api_key = Some(api_key.into()).filter(|key| !key.is_empty());
        Client { api_key, ..self }
    }

    #[must_use]
    pub fn base_url(self, base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        Client { base_url, ..self }
    }

    #[must_use]
    pub fn model(self, model: impl Into<String>) -> Self {
        let model = model.into();
        Client { model, ..self }
    }

    #[must_use]
    pub fn timeout(self, timeout: Duration) -> Self {
        Client { timeout, ..self }
    }

    pub fn set_global(self) {
        *lock_global() = Some(self);
    }

    pub(crate) fn global() -> Client {
        lock_global().get_or_insert_with(Client::from_env).clone()
    }

    pub fn ask<Q: QuestionSet, S: Serialize + ?Sized>(
        &self,
        question_set: Q,
        state: &S,
    ) -> AskFuture<Q> {
        let mut raw_question_set = RawQuestionSet::default();
        question_set.write(&mut raw_question_set);
        let state = json::validate(state).map_err(AskError::State);
        if raw_question_set.is_empty() {
            return AskFuture::new(question_set, async move {
                state?; // a bad state is an error, even with no questions
                Ok(Response::default())
            });
        }

        // serialize now, so the future does not borrow the state
        let body = state.map(|state| self.request_body(&raw_question_set, &state));
        let client = self.clone();
        AskFuture::new(question_set, async move { client.send(body?).await })
    }

    async fn send(self, body: Vec<u8>) -> Result<Response, AskError> {
        let url = self.endpoint()?;
        let api_key = self.api_key.as_deref().ok_or(AskError::MissingApiKey)?;

        let response = self
            .http
            .post(url)
            .bearer_auth(api_key)
            .header(CONTENT_TYPE, "application/json")
            .header(CONTENT_ENCODING, "zstd")
            .timeout(self.timeout)
            .body(compress(&body))
            .send()
            .await
            .map_err(AskError::from_reqwest)?;
        let status = response.status();
        let request_id = request_id(response.headers());
        let text = response.text().await.map_err(AskError::from_reqwest)?;
        if !status.is_success() {
            return Err(AskError::from_status(status, &text, request_id));
        }
        parse_response(&text, request_id)
    }

    #[expect(
        clippy::expect_used,
        reason = "each part is checked json or text, with text keys"
    )]
    fn request_body(&self, raw_question_set: &RawQuestionSet, state: &RawValue) -> Vec<u8> {
        let model = &self.model;
        serde_json::to_vec(&RequestBody {
            model,
            state,
            questions: raw_question_set,
        })
        .expect("the body serializes")
    }

    fn endpoint(&self) -> Result<Url, AskError> {
        let mut url = Url::parse(&self.base_url)
            .ok()
            .filter(|url| matches!(url.scheme(), "http" | "https"))
            .ok_or_else(|| AskError::InvalidBaseUrl {
                url: self.base_url.clone(),
            })?;

        // a proxy can serve the api under a path of its own
        let path = format!("{}/v1/systemone", url.path().trim_end_matches('/'));
        url.set_path(&path);
        Ok(url)
    }
}

#[derive(Debug, Default)]
pub(crate) struct Response {
    pub(crate) raw_answer_set: RawAnswerSet,
    pub(crate) request_id: Option<String>,
}

#[derive(Serialize)]
struct RequestBody<'a> {
    model: &'a str,
    state: &'a RawValue,
    questions: &'a RawQuestionSet,
}

#[derive(Deserialize)]
struct ResponseBody {
    answers: HashMap<String, Value>,
}

fn var(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

fn lock_global() -> MutexGuard<'static, Option<Client>> {
    GLOBAL.lock().unwrap_or_else(PoisonError::into_inner)
}

fn request_id(headers: &HeaderMap) -> Option<String> {
    let id = headers.get(REQUEST_ID)?.to_str().ok()?;
    Some(id.to_owned())
}

fn parse_response(text: &str, request_id: Option<String>) -> Result<Response, AskError> {
    let invalid = |message| AskError::InvalidResponse {
        message,
        request_id: request_id.clone(),
    };
    let response: Value =
        serde_json::from_str(text).map_err(|error| invalid(format!("must be JSON: {error}")))?;
    let response: ResponseBody = serde_json::from_value(response)
        .map_err(|error| invalid(format!("must answer each question: {error}")))?;
    let raw_answer_set = RawAnswerSet::new(response.answers);
    Ok(Response {
        raw_answer_set,
        request_id,
    })
}

#[expect(clippy::expect_used, reason = "zstd fails only when memory runs out")]
fn compress(body: &[u8]) -> Vec<u8> {
    zstd::bulk::compress(body, zstd::DEFAULT_COMPRESSION_LEVEL).expect("zstd has memory")
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::{Client, REQUEST_ID, parse_response, request_id};
    use crate::AskError;
    use crate::ask::wire::RawQuestionSet;
    use reqwest::header::{HeaderMap, HeaderValue};
    use serde_json::value::RawValue;
    use serde_json::{Value, json};
    use std::time::Duration;

    #[test]
    fn endpoint_adds_the_path_of_the_api() {
        let cases = [
            (
                "https://api.typesafe.ai",
                "https://api.typesafe.ai/v1/systemone",
            ),
            (
                "http://localhost:8080/jev",
                "http://localhost:8080/jev/v1/systemone",
            ),
            (
                "http://localhost:8080/jev/",
                "http://localhost:8080/jev/v1/systemone",
            ),
        ];
        for (base_url, url) in cases {
            let client = Client::new().base_url(base_url);
            assert_eq!(client.endpoint().unwrap().as_str(), url, "{base_url}");
        }
    }

    #[test]
    fn endpoint_refuses_a_url_that_is_not_http() {
        for base_url in ["api.typesafe.ai", "ftp://api.typesafe.ai", "localhost:8080"] {
            let error = Client::new().base_url(base_url).endpoint().unwrap_err();
            assert!(
                matches!(error, AskError::InvalidBaseUrl { .. }),
                "{base_url}"
            );
            assert_eq!(
                error.to_string(),
                format!("the base URL must be an absolute HTTP or HTTPS URL, not `{base_url}`")
            );
        }
    }

    fn raw(text: &str) -> Box<RawValue> {
        RawValue::from_string(text.to_owned()).unwrap()
    }

    #[test]
    fn request_body_has_the_model_the_state_and_the_questions() {
        let client = Client::new().model("jev-1.14.0");
        let mut raw_question_set = RawQuestionSet::default();
        raw_question_set.push(&raw(r#"{"type":"noul","instructions":"Is it spam?"}"#));
        let state = raw(r#"{"text":"Buy now!","from":"a"}"#);

        assert_eq!(
            String::from_utf8(client.request_body(&raw_question_set, &state)).unwrap(),
            concat!(
                r#"{"model":"jev-1.14.0","state":{"text":"Buy now!","from":"a"},"#,
                r#""questions":{"q0":{"type":"noul","instructions":"Is it spam?"}}}"#,
            )
        );
    }

    #[test]
    fn request_id_reads_the_header() {
        let cases = [
            (
                Some(HeaderValue::from_static(
                    "req_01a13d1fbca4728f8f7c1cdd312bd3a5",
                )),
                Some("req_01a13d1fbca4728f8f7c1cdd312bd3a5"),
            ),
            (None, None),
            (Some(HeaderValue::from_bytes(b"req_\xff").unwrap()), None),
        ];
        for (value, expected) in cases {
            let mut headers = HeaderMap::new();
            if let Some(value) = value {
                headers.insert(REQUEST_ID, value);
            }

            assert_eq!(request_id(&headers).as_deref(), expected);
        }
    }

    #[test]
    fn parse_response_takes_the_answers_and_the_request_id() {
        let answer = json!({ "type": "noul", "noul": 0.92 });
        let text = json!({ "answers": { "q0": answer } }).to_string();

        let mut response = parse_response(&text, Some("req_1".to_owned())).unwrap();

        assert_eq!(response.raw_answer_set.next::<Value>().unwrap(), answer);
        assert_eq!(response.request_id.as_deref(), Some("req_1"));
    }

    #[test]
    fn parse_response_refuses_text_that_is_not_json() {
        let error = parse_response("<html>", None).unwrap_err();

        assert!(matches!(error, AskError::InvalidResponse { .. }));
        assert_eq!(
            error.to_string(),
            "the response must be JSON: expected value at line 1 column 1"
        );
    }

    #[test]
    fn parse_response_refuses_a_response_with_no_answers() {
        let text = json!({ "model": "jev-1.13.0" }).to_string();

        let error = parse_response(&text, Some("req_1".to_owned())).unwrap_err();

        assert!(matches!(error, AskError::InvalidResponse { .. }));
        assert_eq!(
            error.to_string(),
            "the response must answer each question: missing field `answers` (request ID `req_1`)"
        );
    }

    #[test]
    fn debug_shows_the_settings_but_not_the_key() {
        let client = Client::new().api_key("secret");

        assert_eq!(
            format!("{client:?}"),
            r#"Client { base_url: "https://api.typesafe.ai", model: "jev-latest", timeout: 10s, .. }"#
        );
    }

    #[test]
    fn default_uses_the_default_settings() {
        let client = Client::default();

        assert!(client.api_key.is_none());
        assert_eq!(client.base_url, "https://api.typesafe.ai");
        assert_eq!(client.model, "jev-latest");
        assert_eq!(client.timeout, Duration::from_secs(10));
    }
}
