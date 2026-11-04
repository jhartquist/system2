mod cache;

use crate::ask::wire::{RawAnswerSet, RawQuestionSet};
use crate::{AskError, AskFuture, AskWithReceiptFuture, QuestionSet, Receipt, Usage, json};
use cache::{Cache, PendingLine};
use reqwest::Url;
use reqwest::header::{
    AUTHORIZATION, CONTENT_ENCODING, CONTENT_TYPE, HeaderMap, HeaderValue, RETRY_AFTER,
};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};
use std::{env, fmt};
use tokio::sync::Semaphore;

pub use cache::CacheError;

const BASE_URL: &str = "https://api.typesafe.ai";
const MODEL: &str = "jev-latest";
const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RETRIES: u32 = 2;
const MAX_CONCURRENT_REQUESTS: usize = 8;
const FIRST_BACKOFF: Duration = Duration::from_millis(500);
const LONGEST_BACKOFF: Duration = Duration::from_secs(5);
const LONGEST_RETRY_AFTER: Duration = Duration::from_secs(30);
const JITTER: f64 = 0.25;
const REQUEST_ID: &str = "x-typesafe-request-id";
const RETRY_AFTER_MS: &str = "retry-after-ms";

static GLOBAL: Mutex<Option<Client>> = Mutex::new(None);

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    api_key: Option<String>,
    base_url: String,
    model: String,
    timeout: Duration,
    max_retries: u32,
    limit: Arc<Semaphore>,
    cache: Option<Arc<Cache>>,
    offline: bool,
    usage: Arc<Mutex<Usage>>,
}

impl Client {
    #[must_use]
    #[expect(
        clippy::expect_used,
        clippy::missing_panics_doc,
        reason = "reqwest fails only when TLS cannot start, and `reqwest::Client::new` panics then"
    )]
    pub fn new() -> Self {
        // the api never redirects, and a 301 or 302 turns a post into a get
        let http = reqwest::Client::builder()
            .redirect(Policy::none())
            .build()
            .expect("TLS starts");
        Client {
            http,
            api_key: None,
            base_url: BASE_URL.to_owned(),
            model: MODEL.to_owned(),
            timeout: TIMEOUT,
            max_retries: MAX_RETRIES,
            limit: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
            cache: None,
            offline: false,
            usage: Arc::default(),
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
        let api_key = Some(api_key.into().trim().to_owned()).filter(|key| !key.is_empty());
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

    #[must_use]
    pub fn max_retries(self, max_retries: u32) -> Self {
        Client {
            max_retries,
            ..self
        }
    }

    #[must_use]
    pub fn max_concurrent_requests(self, max_concurrent_requests: usize) -> Self {
        // a limit of 0 stops all requests, and tokio panics past its most permits
        let permits = max_concurrent_requests.clamp(1, Semaphore::MAX_PERMITS);
        let limit = Arc::new(Semaphore::new(permits));
        Client { limit, ..self }
    }

    pub fn cache(self, path: impl Into<PathBuf>) -> Result<Self, CacheError> {
        let cache = Some(Arc::new(Cache::open(path.into())?));
        Ok(Client { cache, ..self })
    }

    #[must_use]
    pub fn offline(self, offline: bool) -> Self {
        Client { offline, ..self }
    }

    #[must_use]
    pub fn usage(&self) -> Usage {
        *self.lock_usage()
    }

    pub fn set_global(self) {
        *lock_global() = Some(self);
    }

    pub(crate) fn global() -> Result<Client, AskError> {
        let mut global = lock_global();
        if let Some(client) = global.as_ref() {
            return Ok(client.clone());
        }
        let client = Client::from_global_env().map_err(AskError::Cache)?;
        Ok(global.insert(client).clone())
    }

    fn from_global_env() -> Result<Client, CacheError> {
        let offline = var("SYSTEM2_OFFLINE").as_deref() == Some("1");
        let client = Client::from_env().offline(offline);
        match var("SYSTEM2_CACHE") {
            Some(path) => client.cache(path),
            None => Ok(client),
        }
    }

    pub fn ask<Q: QuestionSet, S: Serialize + ?Sized>(
        &self,
        mut question_set: Q,
        state: &S,
    ) -> AskFuture<Q> {
        let mut raw_question_set = RawQuestionSet::default();
        question_set.write(&mut raw_question_set);
        let state = json::validate(state).map_err(AskError::State);
        if raw_question_set.is_empty() {
            return AskFuture::new(question_set, async move {
                state?; // a bad state is an error, even with no questions
                Ok(Response::without_request())
            });
        }

        // serialize now, so the future does not borrow the state
        let body = state.map(|state| self.request_body(&raw_question_set, &state));
        let client = self.clone();
        AskFuture::new(question_set, async move { client.send(body?).await })
    }

    pub fn ask_with_receipt<Q: QuestionSet, S: Serialize + ?Sized>(
        &self,
        question_set: Q,
        state: &S,
    ) -> AskWithReceiptFuture<Q> {
        AskWithReceiptFuture::new(self.ask(question_set, state))
    }

    // answers from the cache when it has the request, otherwise sends the request to the api
    async fn send(self, body: Vec<u8>) -> Result<Response, AskError> {
        let cache = self
            .cache
            .clone()
            .map(|cache| (cache, cache::sha256(&body)));

        if let Some((cache, request_sha256)) = &cache
            && let Some(response) = cache.get(request_sha256)
        {
            let ResponseBody { model, answers } = read_response(&response, None)?;

            // the tokens were spent when the response went into the cache
            let receipt = Receipt {
                model,
                usage: Usage::default(),
                cached: true,
                latency: Duration::ZERO,
                request_id: None,
            };
            return Ok(Response {
                raw_answer_set: RawAnswerSet::new(answers),
                receipt,
                pending_line: None,
            });
        }

        if self.offline {
            return Err(AskError::Offline);
        }

        let (text, request_id, latency) = self.post(&body).await?;
        let json = parse_json(&text, request_id.as_deref())?;
        let usage = read_usage(&json, request_id.as_deref())?;

        // before the answers are read, as a response with bad answers still spends its tokens
        self.add_usage(usage);

        let ResponseBody { model, answers } = read_response(&json, request_id.as_deref())?;
        let receipt = Receipt {
            model,
            usage,
            cached: false,
            latency,
            request_id,
        };
        let pending_line =
            cache.map(|(cache, request_sha256)| PendingLine::new(cache, request_sha256, json));
        Ok(Response {
            raw_answer_set: RawAnswerSet::new(answers),
            receipt,
            pending_line,
        })
    }

    // the text, the request ID, and the latency of the response, after the retries
    #[expect(clippy::expect_used, reason = "the client never closes its limit")]
    async fn post(&self, body: &[u8]) -> Result<(String, Option<String>, Duration), AskError> {
        let url = self.endpoint()?;
        let authorization = self.authorization()?;
        let body = compress(body);

        // held through the retries, and a drop of the future gives it back
        let _permit = self.limit.acquire().await.expect("the limit is open");

        // from the first send, so a wait for the limit is not latency
        let started = Instant::now();
        let mut retries = 0;
        loop {
            let (error, retry_after) = match self.attempt(&url, &authorization, &body).await {
                Ok((text, request_id)) => return Ok((text, request_id, started.elapsed())),
                Err(Failure::Final(error)) => return Err(error),
                Err(Failure::Retry { error, retry_after }) => (error, retry_after),
            };
            if retries == self.max_retries {
                return Err(error);
            }
            let Some(wait) = retry_wait(retry_after, retries) else {
                return Err(error);
            };
            tokio::time::sleep(wait).await;
            retries += 1;
        }
    }

    // the text and the request ID of a response with a success status
    async fn attempt(
        &self,
        url: &Url,
        authorization: &HeaderValue,
        body: &[u8],
    ) -> Result<(String, Option<String>), Failure> {
        let response = self
            .http
            .post(url.clone())
            .header(AUTHORIZATION, authorization.clone())
            .header(CONTENT_TYPE, "application/json")
            .header(CONTENT_ENCODING, "zstd")
            .timeout(self.timeout)
            .body(body.to_vec())
            .send()
            .await
            .map_err(Failure::from_reqwest)?;
        let status = response.status();
        let request_id = request_id(response.headers());
        let retry_after = retry_after(response.headers(), SystemTime::now());
        let text = match response.text().await {
            Ok(text) => text,
            Err(error) if status.is_success() => return Err(Failure::from_reqwest(error)),

            // the status and its headers already tell what failed
            Err(_) => String::new(),
        };
        if status.is_success() {
            return Ok((text, request_id));
        }
        let error = AskError::from_status(status, &text, request_id);
        match error {
            AskError::Http {
                status: 408 | 429 | 500..=599,
                ..
            } => Err(Failure::Retry { error, retry_after }),
            _ => Err(Failure::Final(error)),
        }
    }

    fn add_usage(&self, usage: Usage) {
        let mut total = self.lock_usage();
        total.input_tokens = total.input_tokens.saturating_add(usage.input_tokens);
        total.output_tokens = total.output_tokens.saturating_add(usage.output_tokens);
    }

    fn lock_usage(&self) -> MutexGuard<'_, Usage> {
        self.usage.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn authorization(&self) -> Result<HeaderValue, AskError> {
        let api_key = self.api_key.as_deref().ok_or(AskError::MissingApiKey)?;
        let mut authorization = HeaderValue::try_from(format!("Bearer {api_key}"))
            .map_err(|_| AskError::InvalidApiKey)?;
        authorization.set_sensitive(true);
        Ok(authorization)
    }

    #[expect(
        clippy::expect_used,
        reason = "each part is validated json or text, with text keys"
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

#[derive(Debug)]
pub(crate) struct Response {
    pub(crate) raw_answer_set: RawAnswerSet,
    pub(crate) receipt: Receipt,

    // a new response, which `poll` writes to the cache once its answers pass
    pub(crate) pending_line: Option<PendingLine>,
}

impl Response {
    // a set with no questions sends no request, so nothing answers it
    fn without_request() -> Self {
        let receipt = Receipt {
            model: None,
            usage: Usage::default(),
            cached: false,
            latency: Duration::ZERO,
            request_id: None,
        };
        Response {
            raw_answer_set: RawAnswerSet::default(),
            receipt,
            pending_line: None,
        }
    }
}

enum Failure {
    Final(AskError),
    Retry {
        error: AskError,
        retry_after: Option<Duration>,
    },
}

impl Failure {
    fn from_reqwest(error: reqwest::Error) -> Self {
        let error = AskError::from_reqwest(error);
        let retry_after = None;
        Failure::Retry { error, retry_after }
    }
}

#[derive(Serialize)]
struct RequestBody<'a> {
    model: &'a str,
    state: &'a RawValue,
    questions: &'a RawQuestionSet,
}

#[derive(Debug, Deserialize)]
struct ResponseBody {
    model: Option<String>,
    answers: HashMap<String, Value>,
}

fn var(name: &str) -> Option<String> {
    let value = env::var(name).ok()?.trim().to_owned();
    Some(value).filter(|value| !value.is_empty())
}

fn lock_global() -> MutexGuard<'static, Option<Client>> {
    GLOBAL.lock().unwrap_or_else(PoisonError::into_inner)
}

fn request_id(headers: &HeaderMap) -> Option<String> {
    let id = headers.get(REQUEST_ID)?.to_str().ok()?;
    Some(id.to_owned())
}

fn parse_json(text: &str, request_id: Option<&str>) -> Result<Value, AskError> {
    serde_json::from_str(text).map_err(|error| AskError::InvalidResponse {
        message: format!("must be JSON: {error}"),
        request_id: request_id.map(str::to_owned),
    })
}

fn read_response(json: &Value, request_id: Option<&str>) -> Result<ResponseBody, AskError> {
    ResponseBody::deserialize(json).map_err(|error| AskError::InvalidResponse {
        message: format!("must have the shape of the API: {error}"),
        request_id: request_id.map(str::to_owned),
    })
}

// by hand, as a derived `Deserialize` takes an array for a struct; null is the same as no value
fn read_usage(json: &Value, request_id: Option<&str>) -> Result<Usage, AskError> {
    let invalid = |message| AskError::InvalidResponse {
        message,
        request_id: request_id.map(str::to_owned),
    };
    let counts = match &json["usage"] {
        Value::Null => return Ok(Usage::default()),
        Value::Object(counts) => counts,
        other => {
            return Err(invalid(format!(
                "must give `usage` as an object of token counts, not `{other}`"
            )));
        }
    };
    let count = |key: &str| match counts.get(key) {
        None | Some(Value::Null) => Ok(0),
        Some(count) => count.as_u64().ok_or_else(|| {
            invalid(format!(
                "must give `{key}` as a whole number of tokens, not `{count}`"
            ))
        }),
    };
    let input_tokens = count("input_tokens")?;
    let output_tokens = count("output_tokens")?;
    Ok(Usage {
        input_tokens,
        output_tokens,
    })
}

fn retry_after(headers: &HeaderMap, now: SystemTime) -> Option<Duration> {
    let header = |name: &str| headers.get(name)?.to_str().ok();
    let whole_number = |name| header(name)?.parse().ok();
    let date = || httpdate::parse_http_date(header(RETRY_AFTER.as_str())?).ok();
    whole_number(RETRY_AFTER_MS)
        .map(Duration::from_millis)
        .or_else(|| whole_number(RETRY_AFTER.as_str()).map(Duration::from_secs))
        .or_else(|| Some(date()?.duration_since(now).unwrap_or_default()))
}

// none when the server asks for a wait too long to make
fn retry_wait(retry_after: Option<Duration>, retries: u32) -> Option<Duration> {
    match retry_after {
        Some(wait) => (wait <= LONGEST_RETRY_AFTER).then_some(wait),
        None => Some(jitter(backoff(retries))),
    }
}

fn backoff(retries: u32) -> Duration {
    FIRST_BACKOFF
        .saturating_mul(2u32.saturating_pow(retries))
        .min(LONGEST_BACKOFF)
}

fn jitter(wait: Duration) -> Duration {
    wait.mul_f64(1.0 - JITTER * random_fraction())
}

// std has no random function, but each `RandomState` hashes with new keys
fn random_fraction() -> f64 {
    let [a, b, c, d, ..] = RandomState::new().build_hasher().finish().to_le_bytes();
    f64::from(u32::from_le_bytes([a, b, c, d])) / 2f64.powi(32)
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
            .field("max_retries", &self.max_retries)
            .field("cache", &self.cache.as_ref().map(|cache| cache.path()))
            .field("offline", &self.offline)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Client, REQUEST_ID, ResponseBody, backoff, jitter, parse_json, read_response, read_usage,
        request_id, retry_after, retry_wait,
    };
    use crate::AskError;
    use crate::ask::wire::RawQuestionSet;
    use reqwest::header::{HeaderMap, HeaderValue};
    use serde_json::json;
    use serde_json::value::RawValue;
    use std::fs;
    use std::time::Duration;
    use tokio::sync::Semaphore;

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
    fn read_response_takes_the_answers_and_the_model() {
        let answer = json!({ "type": "noul", "noul": 0.92 });
        let cases = [
            (json!("jev-1.13.0"), Some("jev-1.13.0")),
            (json!(null), None),
        ];
        for (model, expected) in cases {
            let json = json!({ "model": model, "answers": { "q0": answer } });

            let ResponseBody { model, answers } = read_response(&json, None).unwrap();

            assert_eq!(answers["q0"], answer);
            assert_eq!(model.as_deref(), expected);
        }
        let ResponseBody { model, .. } = read_response(&json!({ "answers": {} }), None).unwrap();
        assert_eq!(model, None);
    }

    #[test]
    fn parse_json_refuses_text_that_is_not_json() {
        let error = parse_json("<html>", Some("req_1")).unwrap_err();

        assert!(matches!(error, AskError::InvalidResponse { .. }));
        assert_eq!(
            error.to_string(),
            "the response must be JSON: expected value at line 1 column 1 (request ID `req_1`)"
        );
    }

    #[test]
    fn read_response_refuses_a_response_with_no_answers() {
        let json = json!({ "model": "jev-1.13.0" });

        let error = read_response(&json, Some("req_1")).unwrap_err();

        assert!(matches!(error, AskError::InvalidResponse { .. }));
        assert_eq!(
            error.to_string(),
            "the response must have the shape of the API: \
             missing field `answers` (request ID `req_1`)"
        );
    }

    #[test]
    fn read_usage_counts_missing_tokens_as_zero() {
        let cases = [
            (json!({}), (0, 0)),
            (json!({ "usage": null }), (0, 0)),
            (json!({ "usage": {} }), (0, 0)),
            (json!({ "usage": { "input_tokens": 280 } }), (280, 0)),
            (json!({ "usage": { "output_tokens": 1 } }), (0, 1)),
            (
                json!({ "usage": { "input_tokens": null, "output_tokens": 1 } }),
                (0, 1),
            ),
            (
                json!({ "usage": { "input_tokens": 280, "output_tokens": 1 } }),
                (280, 1),
            ),
        ];
        for (json, expected) in cases {
            let usage = read_usage(&json, None).unwrap();

            assert_eq!(
                (usage.input_tokens, usage.output_tokens),
                expected,
                "{json}"
            );
        }
    }

    #[test]
    fn read_usage_refuses_tokens_of_the_wrong_type() {
        let cases = [
            (
                json!("280"),
                "must give `usage` as an object of token counts, not `\"280\"`",
            ),
            (
                json!([280, 1]),
                "must give `usage` as an object of token counts, not `[280,1]`",
            ),
            (
                json!({ "input_tokens": -1 }),
                "must give `input_tokens` as a whole number of tokens, not `-1`",
            ),
            (
                json!({ "output_tokens": 1.5 }),
                "must give `output_tokens` as a whole number of tokens, not `1.5`",
            ),
        ];
        for (usage, message) in cases {
            let error = read_usage(&json!({ "usage": usage }), Some("req_1")).unwrap_err();

            assert!(matches!(error, AskError::InvalidResponse { .. }));
            assert_eq!(
                error.to_string(),
                format!("the response {message} (request ID `req_1`)")
            );
        }
    }

    #[test]
    fn api_key_trims_whitespace() {
        let cases = [(" key\r\n", Some("key")), (" \n", None), ("", None)];
        for (api_key, trimmed) in cases {
            let client = Client::new().api_key(api_key);

            assert_eq!(client.api_key.as_deref(), trimmed, "{api_key:?}");
        }
    }

    #[test]
    fn authorization_hides_the_key() {
        let authorization = Client::new().api_key("secret").authorization().unwrap();

        assert_eq!(authorization, "Bearer secret");
        assert!(authorization.is_sensitive());
    }

    #[test]
    fn retry_after_reads_the_headers() {
        let now = httpdate::parse_http_date("Sat, 31 Oct 2026 12:00:00 GMT").unwrap();
        let cases = [
            (Some("250"), None, Some(Duration::from_millis(250))),
            (None, Some("3"), Some(Duration::from_secs(3))),
            (Some("0"), Some("60"), Some(Duration::ZERO)),
            (Some("1.5"), Some("2"), Some(Duration::from_secs(2))),
            (None, Some("1.5"), None),
            (None, Some("-1"), None),
            (
                None,
                Some("Sat, 31 Oct 2026 12:00:03 GMT"),
                Some(Duration::from_secs(3)),
            ),
            (
                None,
                Some("Sat, 31 Oct 2026 11:59:00 GMT"),
                Some(Duration::ZERO),
            ),
            (None, Some("soon"), None),
            (None, None, None),
        ];
        for (retry_after_ms, retry_after_secs, expected) in cases {
            let mut headers = HeaderMap::new();
            if let Some(value) = retry_after_ms {
                headers.insert("retry-after-ms", HeaderValue::from_static(value));
            }
            if let Some(value) = retry_after_secs {
                headers.insert("retry-after", HeaderValue::from_static(value));
            }

            assert_eq!(
                retry_after(&headers, now),
                expected,
                "{retry_after_ms:?}, {retry_after_secs:?}"
            );
        }
    }

    #[test]
    fn retry_wait_takes_a_header_up_to_30_seconds() {
        let cases = [
            (Duration::ZERO, Some(Duration::ZERO)),
            (Duration::from_secs(30), Some(Duration::from_secs(30))),
            (Duration::from_millis(30_001), None),
        ];
        for (wait, expected) in cases {
            assert_eq!(retry_wait(Some(wait), 0), expected, "{wait:?}");
        }
    }

    #[test]
    fn retry_wait_takes_the_backoff_with_no_header() {
        for retries in 0..6 {
            let wait = retry_wait(None, retries).unwrap();

            let backoff = backoff(retries);
            assert!(
                wait <= backoff && wait >= backoff * 3 / 4,
                "{retries}: {wait:?}"
            );
        }
    }

    #[test]
    fn backoff_doubles_up_to_5_seconds() {
        let cases = [
            (0, Duration::from_millis(500)),
            (1, Duration::from_secs(1)),
            (2, Duration::from_secs(2)),
            (3, Duration::from_secs(4)),
            (4, Duration::from_secs(5)),
            (u32::MAX, Duration::from_secs(5)),
        ];
        for (retries, wait) in cases {
            assert_eq!(backoff(retries), wait, "{retries}");
        }
    }

    #[test]
    fn jitter_subtracts_a_random_part_up_to_a_quarter() {
        let waits: Vec<Duration> = (0..1000).map(|_| jitter(Duration::from_secs(4))).collect();

        // a thousand draws reach near each end of the range
        let shortest = waits.iter().min().unwrap().as_millis();
        let longest = waits.iter().max().unwrap().as_millis();
        assert!((3000..3050).contains(&shortest), "{shortest}");
        assert!((3950..=4000).contains(&longest), "{longest}");
    }

    #[test]
    fn max_concurrent_requests_permits_at_least_one_request() {
        let cases = [(0, 1), (4, 4), (usize::MAX, Semaphore::MAX_PERMITS)];
        for (max_concurrent_requests, permits) in cases {
            let client = Client::new().max_concurrent_requests(max_concurrent_requests);

            assert_eq!(
                client.limit.available_permits(),
                permits,
                "{max_concurrent_requests}"
            );
        }
    }

    #[test]
    fn debug_shows_the_settings_but_not_the_key() {
        let path = std::env::temp_dir().join(format!("system2_debug_{}", std::process::id()));
        let cached = Client::new().cache(&path).unwrap().offline(true);
        let cases = [
            (Client::new(), "None".to_owned(), false),
            (cached, format!("Some({path:?})"), true),
        ];
        for (client, cache, offline) in cases {
            let client = client.api_key("secret");

            assert_eq!(
                format!("{client:?}"),
                format!(
                    "Client {{ base_url: \"https://api.typesafe.ai\", model: \"jev-latest\", \
                     timeout: 10s, max_retries: 2, cache: {cache}, offline: {offline}, .. }}"
                )
            );
        }
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn default_uses_the_default_settings() {
        let client = Client::default();

        assert!(client.api_key.is_none());
        assert_eq!(client.base_url, "https://api.typesafe.ai");
        assert_eq!(client.model, "jev-latest");
        assert_eq!(client.timeout, Duration::from_secs(10));
        assert_eq!(client.max_retries, 2);
        assert_eq!(client.limit.available_permits(), 8);
    }
}
