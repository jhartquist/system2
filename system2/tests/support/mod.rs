#![allow(dead_code, reason = "each test file uses some of the helpers")]
#![allow(clippy::unwrap_used, reason = "test helpers can unwrap")]

use serde_json::{Value, json};
use system2::Client;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

pub(crate) const INSTRUCTIONS: &str = "Is this message spam?";
pub(crate) const STATE: &str = "Buy now!";

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
    ResponseTemplate::new(200).set_body_json(json!({
        "model": "jev-1.13.0",
        "answers": { "q0": { "type": "noul", "noul": probability } },
        "usage": { "input_tokens": 280, "output_tokens": 1 },
    }))
}

pub(crate) fn client_for(server: &MockServer) -> Client {
    Client::new().api_key("key").base_url(server.uri())
}

pub(crate) async fn only_request(server: &MockServer) -> Request {
    let requests = server.received_requests().await.unwrap();
    let [request] = <[Request; 1]>::try_from(requests)
        .unwrap_or_else(|requests| panic!("{} requests", requests.len()));
    request
}

pub(crate) fn request_body(request: &Request) -> Value {
    let body = zstd::decode_all(request.body.as_slice()).unwrap();
    serde_json::from_slice(&body).unwrap()
}
