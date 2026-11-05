#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "the capture of spans can unwrap, as a test helper does"
)]

mod support;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Once;
use support::{
    INSTRUCTIONS, STATE, answer_set_response, cache_path, client_for, mock_api, mock_api_then,
    noul, noul_answer, noul_response, only_request, partial_api, request_text, response,
    sha256_hex,
};
use system2::{AskError, Client, Noul};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use wiremock::ResponseTemplate;

thread_local! {
    static SPANS: RefCell<Vec<TracedSpan>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug)]
struct TracedSpan {
    name: &'static str,
    fields: Fields,
    events: Vec<TracedEvent>,
}

#[derive(Debug)]
struct TracedEvent {
    level: Level,
    target: String,
    fields: Fields,
}

#[derive(Debug, Default)]
struct Fields(BTreeMap<&'static str, String>);

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name(), value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0.insert(field.name(), format!("{value:?}"));
    }
}

// each test polls its requests on its own thread, so a thread local keeps the tests apart
struct Capture;

// the position of a span in `SPANS`
#[derive(Clone, Copy)]
struct Position(usize);

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Capture {
    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, context: Context<'_, S>) {
        let mut fields = Fields::default();
        attributes.record(&mut fields);
        let name = attributes.metadata().name();
        let position = SPANS.with_borrow_mut(|spans| {
            spans.push(TracedSpan {
                name,
                fields,
                events: Vec::new(),
            });
            spans.len() - 1
        });
        let span = context.span(id).unwrap();
        span.extensions_mut().insert(Position(position));
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, context: Context<'_, S>) {
        let span = context.span(id).unwrap();
        let Position(position) = *span.extensions().get().unwrap();
        SPANS.with_borrow_mut(|spans| values.record(&mut spans[position].fields));
    }

    fn on_event(&self, event: &Event<'_>, context: Context<'_, S>) {
        let span = context.event_span(event).expect("each event is in a span");
        let Position(position) = *span.extensions().get().unwrap();
        let mut fields = Fields::default();
        event.record(&mut fields);
        let traced_event = TracedEvent {
            level: *event.metadata().level(),
            target: event.metadata().target().to_owned(),
            fields,
        };
        SPANS.with_borrow_mut(|spans| spans[position].events.push(traced_event));
    }
}

// one subscriber for the process, as tracing caches the interest of each callsite for all threads
fn capture() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let only_system2 = filter_fn(|metadata| metadata.target().starts_with("system2"));
        let subscriber = tracing_subscriber::registry().with(Capture.with_filter(only_system2));
        tracing::subscriber::set_global_default(subscriber).unwrap();
    });
    SPANS.with_borrow_mut(Vec::clear);
}

fn traced_spans() -> Vec<TracedSpan> {
    SPANS.take()
}

fn fields(pairs: &[(&'static str, &str)]) -> BTreeMap<&'static str, String> {
    pairs
        .iter()
        .map(|&(name, value)| (name, value.to_owned()))
        .collect()
}

// each test asks a `Vec`, as the coverage tool counts each type of a generic function alone
fn nouls(instructions: &[&str]) -> Vec<Noul> {
    instructions
        .iter()
        .map(|instructions| noul(instructions))
        .collect()
}

// removes a time, which changes from run to run, and checks that it is a number of milliseconds
fn remove_time(fields: &mut Fields, name: &str) {
    let time = fields.0.remove(name).unwrap();
    assert!(time.parse::<u64>().is_ok(), "{name}: {time}");
}

#[tokio::test]
async fn ask_traces_a_span_with_the_request_and_the_receipt() {
    capture();
    let answers = [&noul_answer(0.92), &noul_answer(0.1)];
    let response = answer_set_response(&answers).insert_header("x-typesafe-request-id", "req_1");
    let server = mock_api(response).await;

    client_for(&server)
        .model("jev-1.14.0")
        .ask(nouls(&[INSTRUCTIONS, "Is it a scam?"]), STATE)
        .await
        .unwrap();

    let request_sha256 = sha256_hex(&request_text(&only_request(&server).await));
    let [mut span] = traced_spans().try_into().unwrap();
    assert_eq!(span.name, "ask");
    remove_time(&mut span.fields, "system2.latency_ms");
    assert_eq!(
        span.fields.0,
        fields(&[
            ("gen_ai.provider.name", "typesafe"),
            ("gen_ai.operation.name", "systemone"),
            ("gen_ai.request.model", "jev-1.14.0"),
            ("system2.question_count", "2"),
            ("system2.request_sha256", &request_sha256),
            ("gen_ai.response.model", "jev-1.13.0"),
            ("system2.cached", "false"),
            ("gen_ai.usage.input_tokens", "280"),
            ("gen_ai.usage.output_tokens", "1"),
            ("system2.request_id", "req_1"),
        ])
    );
}

#[tokio::test]
async fn ask_traces_the_tokens_of_answers_that_fail() {
    capture();
    let server = mock_api(response(&noul_answer(1.5))).await;

    let error = client_for(&server)
        .ask(nouls(&[INSTRUCTIONS]), STATE)
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::InvalidResponse { .. }), "{error}");
    let [span] = traced_spans().try_into().unwrap();
    assert_eq!(span.fields.0["gen_ai.usage.input_tokens"], "280");
    assert_eq!(span.fields.0["gen_ai.usage.output_tokens"], "1");
}

#[tokio::test]
async fn ask_traces_each_attempt_in_the_span() {
    capture();
    let retry_now = ResponseTemplate::new(503).insert_header("retry-after-ms", "0");
    let server = mock_api_then(retry_now, noul_response(0.92)).await;

    client_for(&server)
        .ask(nouls(&[INSTRUCTIONS]), STATE)
        .await
        .unwrap();

    let [span] = traced_spans().try_into().unwrap();
    let [mut retry, mut attempt] = span.events.try_into().unwrap();
    assert_eq!(retry.level, Level::WARN);
    remove_time(&mut retry.fields, "system2.time_ms");
    assert_eq!(
        retry.fields.0,
        fields(&[
            ("message", "retry"),
            ("system2.attempt", "1"),
            ("http.response.status_code", "503"),
            (
                "system2.error",
                "the API refused the request with status 503: Service Unavailable"
            ),
            ("system2.wait_ms", "0"),
        ])
    );
    assert_eq!(attempt.level, Level::DEBUG);
    remove_time(&mut attempt.fields, "system2.time_ms");
    assert_eq!(
        attempt.fields.0,
        fields(&[
            ("message", "attempt"),
            ("system2.attempt", "2"),
            ("http.response.status_code", "200")
        ])
    );
}

#[tokio::test]
async fn ask_traces_the_error_of_the_last_attempt() {
    capture();
    let server = mock_api(ResponseTemplate::new(400)).await;
    let (closed_client, _) = partial_api(b"HTTP/1.1 200 OK\r\n").await;
    let cases = [
        (client_for(&server), Some("400")),
        (closed_client.max_retries(0), None),
    ];
    for (client, status) in cases {
        let error = client.ask(nouls(&[INSTRUCTIONS]), STATE).await.unwrap_err();

        let error = error.to_string();
        let [span] = traced_spans().try_into().unwrap();
        let [mut attempt] = span.events.try_into().unwrap();
        assert_eq!(attempt.level, Level::DEBUG, "{error}");
        remove_time(&mut attempt.fields, "system2.time_ms");
        let mut expected = fields(&[
            ("message", "attempt"),
            ("system2.attempt", "1"),
            ("system2.error", &error),
        ]);
        if let Some(status) = status {
            expected.insert("http.response.status_code", status.to_owned());
        }
        assert_eq!(attempt.fields.0, expected);
    }
}

#[tokio::test]
async fn ask_traces_an_answer_from_the_cache() {
    capture();
    let path = cache_path("ask_traces_an_answer_from_the_cache");
    let server = mock_api(noul_response(0.92)).await;
    let client = client_for(&server).cache(&path).unwrap();
    client.ask(nouls(&[INSTRUCTIONS]), STATE).await.unwrap();
    capture();

    client.ask(nouls(&[INSTRUCTIONS]), STATE).await.unwrap();

    let request_sha256 = sha256_hex(&request_text(&only_request(&server).await));
    let [span] = traced_spans().try_into().unwrap();
    assert!(span.events.is_empty());
    assert_eq!(
        span.fields.0,
        fields(&[
            ("gen_ai.provider.name", "typesafe"),
            ("gen_ai.operation.name", "systemone"),
            ("gen_ai.request.model", "jev-latest"),
            ("system2.question_count", "1"),
            ("system2.request_sha256", &request_sha256),
            ("gen_ai.response.model", "jev-1.13.0"),
            ("system2.cached", "true"),
            ("gen_ai.usage.input_tokens", "0"),
            ("gen_ai.usage.output_tokens", "0"),
            ("system2.latency_ms", "0"),
        ])
    );
}

#[tokio::test]
async fn ask_traces_no_span_without_a_request() {
    capture();
    let client = Client::new();

    client.ask(nouls(&[]), STATE).await.unwrap();
    client.ask(nouls(&[INSTRUCTIONS]), "").await.unwrap_err();

    assert!(traced_spans().is_empty());
}

#[tokio::test]
async fn trace_bodies_traces_each_body_with_its_own_target() {
    capture();
    let path = cache_path("trace_bodies_traces_each_body_with_its_own_target");
    let empty_path = cache_path("trace_bodies_traces_each_body_with_its_own_target_empty");
    let server = mock_api(noul_response(0.92)).await;
    let client = client_for(&server).cache(&path).unwrap();
    let offline_client = client_for(&server)
        .cache(&empty_path)
        .unwrap()
        .offline(true);

    // the first ask sends its request, the second gets its answer from the cache, and the third
    // finds no answer offline
    let cases = [
        (client.clone(), false),
        (client.trace_bodies(true), true),
        (offline_client.trace_bodies(true), true),
    ];
    for (client, trace_bodies) in cases {
        let _ = client.ask(nouls(&[INSTRUCTIONS]), STATE).await;

        let body = request_text(&only_request(&server).await);
        let [span] = traced_spans().try_into().unwrap();
        let bodies: Vec<&TracedEvent> = span
            .events
            .iter()
            .filter(|event| event.target == "system2::body")
            .collect();
        if trace_bodies {
            let [traced_body] = bodies.try_into().unwrap();
            assert_eq!(traced_body.level, Level::TRACE);
            assert_eq!(
                traced_body.fields.0,
                fields(&[("message", "body"), ("system2.body", &body)])
            );
        } else {
            assert!(bodies.is_empty());
        }
    }
}
