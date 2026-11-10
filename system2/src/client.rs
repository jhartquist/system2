mod cache;

use crate::ask::wire::{RawAnswerSet, RawQuestionSet};
use crate::image::{self, Bytes};
use crate::{AskError, AskFuture, AskWithReceiptFuture, QuestionSet, Receipt, Usage, json};
use cache::{Cache, PendingLine};
use reqwest::Url;
use reqwest::header::{
    AUTHORIZATION, CONTENT_ENCODING, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, RETRY_AFTER,
};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use serde_json::value::RawValue;
use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};
use std::{env, fmt, future, iter};
use tokio::sync::Semaphore;
use tracing::field::Empty;
use tracing::{Instrument, Span, debug, info_span, trace, warn};

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
const LONG_CONTEXT_HEADER: HeaderName =
    HeaderName::from_static("typesafe-danger-zone-long-context");
const IMAGES_HEADER: HeaderName = HeaderName::from_static("typesafe-danger-zone-images");
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

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
    long_context: bool,
    compression: Compression,
    cache: Option<Arc<Cache>>,
    offline: bool,
    trace_bodies: bool,
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
            long_context: false,
            compression: Compression::default(),
            cache: None,
            offline: false,
            trace_bodies: false,
            usage: Arc::default(),
        }
    }

    pub fn from_env() -> Result<Self, EnvError> {
        let api_key = var("TYPESAFE_API_KEY");
        if let Some(api_key) = &api_key
            && authorization_for(api_key).is_none()
        {
            return Err(EnvError::InvalidApiKey);
        }
        let base_url = var("TYPESAFE_BASE_URL").unwrap_or_else(|| BASE_URL.to_owned());
        if endpoint_for(&base_url).is_none() {
            return Err(EnvError::InvalidBaseUrl { url: base_url });
        }
        let model = var("TYPESAFE_DEFAULT_MODEL").unwrap_or_else(|| MODEL.to_owned());
        let offline = var("SYSTEM2_OFFLINE").as_deref() == Some("1");
        let cache = match var("SYSTEM2_CACHE") {
            Some(path) => Some(Arc::new(Cache::open(path.into()).map_err(EnvError::Cache)?)),
            None => None,
        };
        Ok(Client {
            api_key,
            base_url,
            model,
            cache,
            offline,
            ..Client::new()
        })
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

    #[must_use]
    pub fn long_context(self, long_context: bool) -> Self {
        Client {
            long_context,
            ..self
        }
    }

    #[must_use]
    pub fn compression(self, compression: Compression) -> Self {
        Client {
            compression,
            ..self
        }
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
    pub fn trace_bodies(self, trace_bodies: bool) -> Self {
        Client {
            trace_bodies,
            ..self
        }
    }

    #[must_use]
    pub fn usage(&self) -> Usage {
        *self.lock_usage()
    }

    #[expect(
        clippy::must_use_candidate,
        clippy::return_self_not_must_use,
        reason = "a caller can set the global client and drop the clone"
    )]
    pub fn set_global(self) -> Client {
        lock_global().insert(self).clone()
    }

    pub fn global() -> Result<Client, EnvError> {
        let mut global = lock_global();
        if let Some(client) = global.as_ref() {
            return Ok(client.clone());
        }
        let client = Client::from_env()?;
        Ok(global.insert(client).clone())
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
        let state = match state {
            Ok(state) => state,
            Err(error) => return AskFuture::new(question_set, future::ready(Err(error))),
        };

        // looked up now, so the caller can drop an `Image` once `ask` returns
        let references = image::references(iter::once(state.get()).chain(raw_question_set.texts()));
        let images = find_images(&references);
        let body = self.request_body(&raw_question_set, &state, &references);
        let has_bbox = raw_question_set.has_bbox();
        let request_sha256 = cache::sha256(body.as_bytes());
        let span = info_span!(
            "ask",
            gen_ai.provider.name = "typesafe",
            gen_ai.operation.name = "systemone",
            gen_ai.request.model = self.model,
            system2.question_count = raw_question_set.len(),
            system2.request_sha256 = hex::encode(request_sha256),
            gen_ai.response.model = Empty,
            gen_ai.usage.input_tokens = Empty,
            gen_ai.usage.output_tokens = Empty,
            system2.cached = Empty,
            system2.latency_ms = Empty,
            system2.request_id = Empty,
        );
        let request = self
            .clone()
            .send(body, request_sha256, images, has_bbox)
            .instrument(span);
        AskFuture::new(question_set, request)
    }

    pub fn ask_with_receipt<Q: QuestionSet, S: Serialize + ?Sized>(
        &self,
        question_set: Q,
        state: &S,
    ) -> AskWithReceiptFuture<Q> {
        AskWithReceiptFuture::new(self.ask(question_set, state))
    }

    // answers from the cache when it has the request, otherwise sends the request to the api
    async fn send(
        self,
        body: String,
        request_sha256: [u8; 32],
        images: Result<Vec<(String, Bytes)>, AskError>,
        has_bbox: bool,
    ) -> Result<Response, AskError> {
        if self.trace_bodies {
            trace!(target: "system2::body", { system2.body = body }, "body");
        }

        // before the cache, so a run from the cache fails where a live run does
        let images = images?;

        if let Some(cache) = &self.cache
            && let Some(response) = cache.get(&request_sha256)
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
            record(&receipt);
            return Ok(Response {
                raw_answer_set: RawAnswerSet::new(answers),
                receipt,
                pending_line: None,
            });
        }

        if self.offline {
            return Err(AskError::Offline);
        }
        if let Some(cache) = &self.cache {
            cache.check_writable().map_err(AskError::Cache)?;
        }

        let (text, request_id, latency) =
            self.post(body, request_sha256, &images, has_bbox).await?;
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
        record(&receipt);
        let pending_line = self
            .cache
            .map(|cache| PendingLine::new(cache, request_sha256, json));
        Ok(Response {
            raw_answer_set: RawAnswerSet::new(answers),
            receipt,
            pending_line,
        })
    }

    // the text, the request ID, and the latency of the response, after the retries
    #[expect(clippy::expect_used, reason = "the client never closes its limit")]
    async fn post(
        &self,
        body: String,
        request_sha256: [u8; 32],
        images: &[(String, Bytes)],
        has_bbox: bool,
    ) -> Result<(String, Option<String>, Duration), AskError> {
        let url = self.endpoint()?;

        // the hash of the json, which no part of the body can hold
        let boundary = (!images.is_empty()).then(|| hex::encode(request_sha256));
        let headers = self.headers(boundary.as_deref(), has_bbox)?;
        let bbox_model = has_bbox.then_some(self.model.as_str());
        let body = match &boundary {
            Some(boundary) => multipart(boundary, &body, images)?,
            None => body.into_bytes(),
        };

        let body = match self.compression {
            Compression::None => body,
            Compression::Zstd => compress(&body),
        };

        // held through the retries, and a drop of the future gives it back
        let _permit = self.limit.acquire().await.expect("the limit is open");

        // from the first send, so a wait for the limit is not latency
        let started = Instant::now();
        let mut retries = 0;
        loop {
            let attempt = retries + 1;
            let attempt_started = Instant::now();
            let (status, result) = self.attempt(&url, &headers, &body, bbox_model).await;
            let time_ms = millis(attempt_started.elapsed());
            let (error, wait) = match result {
                Ok((text, request_id)) => {
                    debug!(
                        system2.attempt = attempt,
                        http.response.status_code = status,
                        system2.time_ms = time_ms,
                        "attempt"
                    );
                    return Ok((text, request_id, started.elapsed()));
                }
                Err(Failure::Retry { error, retry_after }) if retries < self.max_retries => {
                    (error, retry_wait(retry_after, retries))
                }
                Err(Failure::Final(error) | Failure::Retry { error, .. }) => (error, None),
            };
            let Some(wait) = wait else {
                debug!(
                    system2.attempt = attempt,
                    http.response.status_code = status,
                    system2.time_ms = time_ms,
                    system2.error = &error as &dyn Error,
                    "attempt"
                );
                return Err(error);
            };
            warn!(
                system2.attempt = attempt,
                http.response.status_code = status,
                system2.time_ms = time_ms,
                system2.error = &error as &dyn Error,
                system2.wait_ms = millis(wait),
                "retry"
            );
            tokio::time::sleep(wait).await;
            retries += 1;
        }
    }

    // the status of the response, if one came, and the result of the attempt
    async fn attempt(
        &self,
        url: &Url,
        headers: &HeaderMap,
        body: &[u8],
        bbox_model: Option<&str>,
    ) -> (Option<u16>, Result<(String, Option<String>), Failure>) {
        let sent = self
            .http
            .post(url.clone())
            .headers(headers.clone())
            .timeout(self.timeout)
            .body(body.to_vec())
            .send()
            .await;
        match sent {
            Ok(response) => (
                Some(response.status().as_u16()),
                read_success(response, self.long_context, bbox_model).await,
            ),
            Err(error) => (None, Err(Failure::from_reqwest(error))),
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

    // a boundary for a multipart body, which a request with images has
    fn headers(&self, boundary: Option<&str>, has_bbox: bool) -> Result<HeaderMap, AskError> {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, self.authorization()?);

        let content_type = match boundary {
            Some(boundary) => content_type_for(boundary),
            None => HeaderValue::from_static("application/json"),
        };
        headers.insert(CONTENT_TYPE, content_type);

        if boundary.is_some() || has_bbox {
            headers.insert(IMAGES_HEADER, HeaderValue::from_static("true"));
        }
        if self.compression == Compression::Zstd {
            headers.insert(CONTENT_ENCODING, HeaderValue::from_static("zstd"));
        }
        if self.long_context {
            headers.insert(LONG_CONTEXT_HEADER, HeaderValue::from_static("true"));
        }
        Ok(headers)
    }

    fn authorization(&self) -> Result<HeaderValue, AskError> {
        let api_key = self.api_key.as_deref().ok_or(AskError::MissingApiKey)?;
        authorization_for(api_key).ok_or(AskError::InvalidApiKey)
    }

    #[expect(
        clippy::expect_used,
        reason = "each part is validated json or text, with text keys"
    )]
    fn request_body(
        &self,
        raw_question_set: &RawQuestionSet,
        state: &RawValue,
        references: &BTreeSet<String>,
    ) -> String {
        let model = &self.model;
        serde_json::to_string(&RequestBody {
            model,
            state,
            questions: raw_question_set,
            assets: Assets(references),
        })
        .expect("the body serializes")
    }

    fn endpoint(&self) -> Result<Url, AskError> {
        endpoint_for(&self.base_url).ok_or_else(|| AskError::InvalidBaseUrl {
            url: self.base_url.clone(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EnvError {
    #[error(
        "`TYPESAFE_API_KEY` must be a valid HTTP header value: \
         remove line breaks and other control characters"
    )]
    InvalidApiKey,
    #[error("`TYPESAFE_BASE_URL` must be an absolute HTTP or HTTPS URL, not `{url}`")]
    InvalidBaseUrl { url: String },
    #[error(transparent)]
    Cache(CacheError),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Compression {
    None,
    #[default]
    Zstd,
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
    #[serde(skip_serializing_if = "Assets::is_empty")]
    assets: Assets<'a>,
}

// the part of each image, by its reference
struct Assets<'a>(&'a BTreeSet<String>);

impl Assets<'_> {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for Assets<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Asset<'a> {
            part: &'a str,
        }
        let assets = self
            .0
            .iter()
            .map(|reference| (reference, Asset { part: reference }));
        serializer.collect_map(assets)
    }
}

#[derive(Debug, Deserialize)]
struct ResponseBody {
    model: Option<String>,
    answers: HashMap<String, Value>,
}

fn find_images(references: &BTreeSet<String>) -> Result<Vec<(String, Bytes)>, AskError> {
    references
        .iter()
        .map(|reference| match image::find(reference) {
            Some(bytes) => Ok((reference.clone(), bytes)),
            None => Err(AskError::MissingImage {
                reference: reference.clone(),
            }),
        })
        .collect()
}

#[expect(clippy::expect_used, reason = "a boundary is hex digits")]
fn content_type_for(boundary: &str) -> HeaderValue {
    HeaderValue::try_from(format!("multipart/form-data; boundary={boundary}"))
        .expect("the content type is a header value")
}

// by hand, as reqwest sends a form as a stream, and zstd compresses the whole body
fn multipart(boundary: &str, json: &str, images: &[(String, Bytes)]) -> Result<Vec<u8>, AskError> {
    let size = images.iter().map(|(_, bytes)| bytes.as_slice().len()).sum();
    if size > MAX_IMAGE_BYTES {
        return Err(AskError::ByteLimit { size });
    }

    let mut body = Vec::new();
    let request_headers =
        "Content-Disposition: form-data; name=\"request\"\r\nContent-Type: application/json";
    add_part(&mut body, boundary, request_headers, json.as_bytes());

    for (reference, bytes) in images {
        let headers = format!(
            "Content-Disposition: form-data; name=\"{reference}\"; filename=\"{reference}\""
        );
        add_part(&mut body, boundary, &headers, bytes.as_slice());
    }

    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok(body)
}

fn add_part(body: &mut Vec<u8>, boundary: &str, headers: &str, content: &[u8]) {
    body.extend_from_slice(format!("--{boundary}\r\n{headers}\r\n\r\n").as_bytes());
    body.extend_from_slice(content);
    body.extend_from_slice(b"\r\n");
}

fn var(name: &str) -> Option<String> {
    let value = env::var(name).ok()?.trim().to_owned();
    Some(value).filter(|value| !value.is_empty())
}

fn lock_global() -> MutexGuard<'static, Option<Client>> {
    GLOBAL.lock().unwrap_or_else(PoisonError::into_inner)
}

// the header of a key, or none for a key that the http library cannot send
fn authorization_for(api_key: &str) -> Option<HeaderValue> {
    let mut authorization = HeaderValue::try_from(format!("Bearer {api_key}")).ok()?;
    authorization.set_sensitive(true);
    Some(authorization)
}

// the url of the api, or none for a base url that is not absolute http or https
fn endpoint_for(base_url: &str) -> Option<Url> {
    let mut url = Url::parse(base_url)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))?;

    // a proxy can serve the api under a path of its own
    let path = format!("{}/v1/systemone", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Some(url)
}

// the text and the request ID of a response with a success status
async fn read_success(
    response: reqwest::Response,
    long_context: bool,
    bbox_model: Option<&str>,
) -> Result<(String, Option<String>), Failure> {
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
    let error = AskError::from_status(status, &text, request_id, long_context, bbox_model);
    match error {
        AskError::Http {
            status: 408 | 429 | 500..=599,
            ..
        } => Err(Failure::Retry { error, retry_after }),
        _ => Err(Failure::Final(error)),
    }
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

// writes the receipt into the `ask` span of the request
fn record(receipt: &Receipt) {
    let span = Span::current();
    span.record("gen_ai.response.model", receipt.model.as_deref());
    span.record("gen_ai.usage.input_tokens", receipt.usage.input_tokens);
    span.record("gen_ai.usage.output_tokens", receipt.usage.output_tokens);
    span.record("system2.cached", receipt.cached);
    span.record("system2.latency_ms", millis(receipt.latency));
    span.record("system2.request_id", receipt.request_id.as_deref());
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
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
            .field("long_context", &self.long_context)
            .field("compression", &self.compression)
            .field("cache", &self.cache.as_ref().map(|cache| cache.path()))
            .field("offline", &self.offline)
            .field("trace_bodies", &self.trace_bodies)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BTreeSet, Bytes, Client, Compression, REQUEST_ID, ResponseBody, backoff, jitter, multipart,
        parse_json, read_response, read_usage, request_id, retry_after, retry_wait,
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
            client.request_body(&raw_question_set, &state, &BTreeSet::new()),
            concat!(
                r#"{"model":"jev-1.14.0","state":{"text":"Buy now!","from":"a"},"#,
                r#""questions":{"q0":{"type":"noul","instructions":"Is it spam?"}}}"#,
            )
        );
    }

    #[test]
    fn request_body_maps_each_reference_to_its_part() {
        let client = Client::new().model("jev-1.14.0");
        let mut raw_question_set = RawQuestionSet::default();
        raw_question_set.push(&raw(r#"{"type":"noul","instructions":"Is it red?"}"#));
        let state = raw(r#""s2-img-0123456789abcdef and s2-img-fedcba9876543210""#);
        let references = BTreeSet::from(
            ["s2-img-fedcba9876543210", "s2-img-0123456789abcdef"].map(str::to_owned),
        );

        assert_eq!(
            client.request_body(&raw_question_set, &state, &references),
            concat!(
                r#"{"model":"jev-1.14.0","#,
                r#""state":"s2-img-0123456789abcdef and s2-img-fedcba9876543210","#,
                r#""questions":{"q0":{"type":"noul","instructions":"Is it red?"}},"#,
                r#""assets":{"s2-img-0123456789abcdef":{"part":"s2-img-0123456789abcdef"},"#,
                r#""s2-img-fedcba9876543210":{"part":"s2-img-fedcba9876543210"}}}"#,
            )
        );
    }

    #[test]
    fn multipart_puts_the_json_then_each_image_in_a_part() {
        let images = [
            (
                "s2-img-0123456789abcdef".to_owned(),
                Bytes::Static(b"\x89PNG"),
            ),
            (
                "s2-img-fedcba9876543210".to_owned(),
                Bytes::Static(b"\xFF\xD8"),
            ),
        ];

        let body = multipart("b0", r#"{"model":"jev-latest"}"#, &images).unwrap();

        let expected = [
            &b"--b0\r\n"[..],
            b"Content-Disposition: form-data; name=\"request\"\r\n",
            b"Content-Type: application/json\r\n\r\n",
            br#"{"model":"jev-latest"}"#,
            b"\r\n--b0\r\n",
            b"Content-Disposition: form-data; name=\"s2-img-0123456789abcdef\"; ",
            b"filename=\"s2-img-0123456789abcdef\"\r\n\r\n",
            b"\x89PNG",
            b"\r\n--b0\r\n",
            b"Content-Disposition: form-data; name=\"s2-img-fedcba9876543210\"; ",
            b"filename=\"s2-img-fedcba9876543210\"\r\n\r\n",
            b"\xFF\xD8",
            b"\r\n--b0--\r\n",
        ]
        .concat();
        assert_eq!(body, expected);
    }

    #[test]
    fn multipart_refuses_images_over_32_mib() {
        let bytes: &'static [u8] = vec![0; 16 * 1024 * 1024 + 1].leak();
        let images = [
            ("s2-img-0123456789abcdef".to_owned(), Bytes::Static(bytes)),
            (
                "s2-img-fedcba9876543210".to_owned(),
                Bytes::Static(&bytes[1..]),
            ),
        ];

        let error = multipart("b0", "{}", &images).unwrap_err();

        assert!(matches!(error, AskError::ByteLimit { size: 33_554_433 }));
        let at_the_limit = [images[1].clone(), images[1].clone()];
        assert!(multipart("b0", "{}", &at_the_limit).is_ok());
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
                     timeout: 10s, max_retries: 2, long_context: false, compression: Zstd, \
                     cache: {cache}, offline: {offline}, trace_bodies: false, .. }}"
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
        assert!(!client.long_context);
        assert_eq!(client.compression, Compression::Zstd);
        assert!(!client.trace_bodies);
    }
}
