#![allow(dead_code, reason = "each test file uses some of the helpers")]
#![allow(clippy::unwrap_used, reason = "test helpers can unwrap")]

use serde_json::value::RawValue;
use serde_json::{Map, Value, json};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use system2::{Client, Noul};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

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

pub(crate) async fn mock_api_then(first: ResponseTemplate, then: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(first)
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount(&server, then).await;
    server
}

pub(crate) async fn mount(server: &MockServer, response: impl Respond + 'static) {
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(response)
        .mount(server)
        .await;
}

// answers each question by its type: a noul at 0.95, a choice with the first option by name, and
// a score with the highest level, each certain so that its probabilities agree
pub(crate) struct AnswerByType;

impl Respond for AnswerByType {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let questions = &request_body(request)["questions"];
        let answers: Vec<Value> = (0..questions.as_object().unwrap().len())
            .map(|index| answer_by_type(&questions[format!("q{index}")]))
            .collect();
        answer_set_response(&answers.iter().collect::<Vec<_>>())
    }
}

fn answer_by_type(question: &Value) -> Value {
    let criteria = &question["criteria"];
    match question["type"].as_str().unwrap() {
        "noul" => noul_answer(0.95),
        "choice" => {
            // serde_json sorts the keys, so the first name is the first in sorted order
            let names: Vec<&String> = criteria.as_object().unwrap().keys().collect();
            json!({
                "type": "choice",
                "choice": names[0],
                "confidence": 1.0,
                "probabilities": certain(names),
            })
        }
        "score" => {
            let positions: Vec<String> = (0..criteria.as_array().unwrap().len())
                .rev()
                .map(|position| position.to_string())
                .collect();
            json!({
                "type": "score",
                "score": positions.len() - 1,
                "confidence": 1.0,
                "probabilities": certain(positions),
            })
        }
        other => panic!("unknown question type {other}"),
    }
}

// the first key is certain, and each other key has no chance
fn certain<K: ToString>(keys: impl IntoIterator<Item = K>) -> Map<String, Value> {
    keys.into_iter()
        .enumerate()
        .map(|(index, key)| (key.to_string(), json!(if index == 0 { 1.0 } else { 0.0 })))
        .collect()
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

// other programs on the machine can probe a new local port, so only requests to the api count
pub(crate) async fn api_requests(server: &MockServer) -> Vec<Request> {
    let mut requests = server.received_requests().await.unwrap();
    requests.retain(|request| request.method == "POST" && request.url.path() == "/v1/systemone");
    requests
}

// skips the connections of other programs, as `api_requests` does
pub(crate) async fn accept_api_request(listener: &TcpListener) -> TcpStream {
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut start = [0; 5];
        if socket.read_exact(&mut start).await.is_ok() && &start == b"POST " {
            return socket;
        }
    }
}

// a server that sends this start of a response to each request, then closes the connection
pub(crate) async fn partial_api(partial_response: &'static [u8]) -> (Client, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let connections = Arc::new(AtomicUsize::new(0));
    let accepted = Arc::clone(&connections);
    tokio::spawn(async move {
        loop {
            let mut socket = accept_api_request(&listener).await;
            accepted.fetch_add(1, Ordering::SeqCst);
            socket.write_all(partial_response).await.unwrap();

            // windows resets a socket that closes with unread data, and the reset can reach the
            // client before the response, so close the write side and read the rest
            tokio::spawn(async move {
                let _ = socket.shutdown().await;
                let _ = socket.read_to_end(&mut Vec::new()).await;
            });
        }
    });
    let client = Client::new().api_key("key").base_url(base_url);
    (client, connections)
}

pub(crate) async fn no_request_sent(server: &MockServer) -> bool {
    api_requests(server).await.is_empty()
}

pub(crate) async fn request_count(server: &MockServer) -> usize {
    api_requests(server).await.len()
}

pub(crate) async fn only_request(server: &MockServer) -> Request {
    let mut requests = api_requests(server).await;
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
