use crate::{AskError, Noul, NoulAnswer, json};
use reqwest::Url;
use reqwest::header::{CONTENT_ENCODING, CONTENT_TYPE, HeaderMap};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt;
use std::time::Duration;

const BASE_URL: &str = "https://api.typesafe.ai";
const MODEL: &str = "jev-latest";
const TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_ID: &str = "x-typesafe-request-id";

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

    pub async fn ask<S: Serialize + ?Sized>(
        &self,
        question: Noul,
        state: &S,
    ) -> Result<NoulAnswer, AskError> {
        let state = json::validate(state).map_err(AskError::State)?;
        let url = self.endpoint()?;
        let api_key = self.api_key.as_deref().ok_or(AskError::MissingApiKey)?;
        let body = self.body(&question, &state);

        let response = self
            .http
            .post(url)
            .bearer_auth(api_key)
            .header(CONTENT_TYPE, "application/json")
            .header(CONTENT_ENCODING, "zstd")
            .timeout(self.timeout)
            .body(compress(body.to_string().as_bytes()))
            .send()
            .await
            .map_err(AskError::from_reqwest)?;
        let status = response.status();
        let request_id = request_id(response.headers());
        let text = response.text().await.map_err(AskError::from_reqwest)?;
        if !status.is_success() {
            return Err(AskError::from_status(status, &text, request_id));
        }
        parse_answer(&text, request_id)
    }

    fn body(&self, question: &Noul, state: &Value) -> Value {
        json!({
            "model": self.model,
            "state": state,
            "questions": { "q0": question.to_json() },
        })
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

#[derive(Deserialize)]
struct Response {
    answers: Answers,
}

#[derive(Deserialize)]
struct Answers {
    q0: Answer,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Answer {
    Noul { noul: f64 },
}

fn request_id(headers: &HeaderMap) -> Option<String> {
    let id = headers.get(REQUEST_ID)?.to_str().ok()?;
    Some(id.to_owned())
}

fn parse_answer(text: &str, request_id: Option<String>) -> Result<NoulAnswer, AskError> {
    let response: Value = serde_json::from_str(text).map_err(|error| {
        let message = format!("must be JSON: {error}");
        AskError::InvalidResponse {
            message,
            request_id: request_id.clone(),
        }
    })?;
    let Response {
        answers: Answers {
            q0: Answer::Noul { noul },
        },
    } = serde_json::from_value(response).map_err(|error| {
        let message = format!("must answer each question: {error}");
        AskError::InvalidResponse {
            message,
            request_id,
        }
    })?;
    Ok(NoulAnswer::new(noul))
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
    use super::{Client, REQUEST_ID, parse_answer, request_id};
    use crate::{AskError, Noul};
    use reqwest::header::{HeaderMap, HeaderValue};
    use serde_json::json;

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
            assert_eq!(client.endpoint().unwrap().as_str(), url);
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

    #[test]
    fn body_has_the_model_the_state_and_the_question() {
        let client = Client::new().model("jev-1.14.0");
        let noul = Noul::new("Is it spam?").unwrap();

        assert_eq!(
            client.body(&noul, &json!("Buy now!")),
            json!({
                "model": "jev-1.14.0",
                "state": "Buy now!",
                "questions": { "q0": { "type": "noul", "instructions": "Is it spam?" } },
            })
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
    #[expect(clippy::float_cmp, reason = "the answer keeps the value of the api")]
    fn parse_answer_takes_a_noul_answer() {
        let text = json!({ "answers": { "q0": { "type": "noul", "noul": 0.92 } } }).to_string();

        assert_eq!(parse_answer(&text, None).unwrap().probability(), 0.92);
    }

    #[test]
    fn parse_answer_refuses_text_that_is_not_json() {
        let error = parse_answer("<html>", None).unwrap_err();

        assert!(matches!(error, AskError::InvalidResponse { .. }));
        assert_eq!(
            error.to_string(),
            "the response must be JSON: expected value at line 1 column 1"
        );
    }

    #[test]
    fn parse_answer_refuses_the_wrong_answer() {
        let text = json!({ "answers": { "q0": { "type": "box", "confidence": 0.9 } } });

        let error = parse_answer(&text.to_string(), None).unwrap_err();

        assert!(matches!(error, AskError::InvalidResponse { .. }));
        assert_eq!(
            error.to_string(),
            "the response must answer each question: unknown variant `box`, expected `noul`"
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
    fn default_is_new() {
        assert_eq!(
            format!("{:?}", Client::default()),
            format!("{:?}", Client::new())
        );
    }
}
