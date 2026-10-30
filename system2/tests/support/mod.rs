#![allow(dead_code, reason = "each test file uses some of the helpers")]
#![allow(clippy::unwrap_used, reason = "test helpers can unwrap")]

use serde_json::value::RawValue;
use serde_json::{Map, Value, json};
use std::cell::Cell;
use std::collections::HashMap;
use system2::{Client, Noul};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

pub(crate) const INSTRUCTIONS: &str = "Is this message spam?";
pub(crate) const STATE: &str = "Buy now!";

thread_local! {
    static COUNTED_DROPS: Cell<usize> = const { Cell::new(0) };
}

// a value that counts its drops, so a test sees when a question makes and drops its values
#[derive(Debug)]
pub(crate) struct Counted;

impl Drop for Counted {
    fn drop(&mut self) {
        COUNTED_DROPS.set(COUNTED_DROPS.get() + 1);
    }
}

pub(crate) fn counted_drops() -> usize {
    COUNTED_DROPS.get()
}

pub(crate) async fn mock_api(response: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    mount(&server, response).await;
    server
}

pub(crate) async fn mount(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(response)
        .mount(server)
        .await;
}

pub(crate) fn noul_response(probability: f64) -> ResponseTemplate {
    response(&noul_answer(probability))
}

pub(crate) fn response(answer: &Value) -> ResponseTemplate {
    answer_set_response(&[answer])
}

pub(crate) fn answer_set_response(answers: &[&Value]) -> ResponseTemplate {
    let answers: Map<String, Value> = answers
        .iter()
        .enumerate()
        .map(|(index, &answer)| (format!("q{index}"), answer.clone()))
        .collect();
    ResponseTemplate::new(200).set_body_json(json!({
        "model": "jev-1.13.0",
        "answers": answers,
        "usage": { "input_tokens": 280, "output_tokens": 1 },
    }))
}

pub(crate) fn noul_answer(probability: f64) -> Value {
    json!({ "type": "noul", "noul": probability })
}

pub(crate) fn noul(instructions: &str) -> Noul {
    Noul::new(instructions).unwrap()
}

pub(crate) fn client_for(server: &MockServer) -> Client {
    Client::new().api_key("key").base_url(server.uri())
}

pub(crate) async fn no_request_sent(server: &MockServer) -> bool {
    server
        .received_requests()
        .await
        .is_some_and(|requests| requests.is_empty())
}

pub(crate) async fn only_request(server: &MockServer) -> Request {
    let mut requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "expected exactly one request");
    requests.pop().unwrap()
}

pub(crate) fn request_body(request: &Request) -> Value {
    serde_json::from_str(&request_text(request)).unwrap()
}

pub(crate) fn request_text(request: &Request) -> String {
    let body = zstd::decode_all(request.body.as_slice()).unwrap();
    String::from_utf8(body).unwrap()
}

pub(crate) fn instructions(request: &Request) -> Vec<Value> {
    let questions = &request_body(request)["questions"];
    (0..questions.as_object().unwrap().len())
        .map(|index| questions[format!("q{index}")]["instructions"].clone())
        .collect()
}

pub(crate) fn question_texts(request: &Request) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        questions: HashMap<String, Box<RawValue>>,
    }
    let body: Body = serde_json::from_str(&request_text(request)).unwrap();
    (0..body.questions.len())
        .map(|index| body.questions[&format!("q{index}")].get().to_owned())
        .collect()
}
