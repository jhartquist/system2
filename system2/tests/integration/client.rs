use serde_json::{Value, json};
use std::error::Error;
use std::time::Duration;
use system2::{AskError, Client, InvalidJson, Noul};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const SPAM: &str = "Is this message spam?";
const MESSAGE: &str = "Buy now!";

async fn mock_api(response: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

fn response_with(answer: &Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "model": "jev-1.13.0",
        "answers": { "q0": answer },
        "usage": { "input_tokens": 280, "output_tokens": 1 },
    }))
}

fn noul_response(probability: f64) -> ResponseTemplate {
    response_with(&json!({ "type": "noul", "noul": probability }))
}

fn client_for(server: &MockServer) -> Client {
    Client::new().api_key("key").base_url(server.uri())
}

async fn only_request(server: &MockServer) -> Request {
    let mut requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    requests.remove(0)
}

fn request_json(request: &Request) -> Value {
    let json = zstd::decode_all(request.body.as_slice()).unwrap();
    serde_json::from_slice(&json).unwrap()
}

async fn no_request_sent(server: &MockServer) -> bool {
    server.received_requests().await.unwrap().is_empty()
}

#[tokio::test]
#[expect(clippy::float_cmp, reason = "the answer keeps the value of the api")]
async fn ask_reads_the_probability() {
    let server = mock_api(noul_response(0.92)).await;

    let answer = client_for(&server)
        .ask(Noul::new(SPAM).unwrap(), MESSAGE)
        .await
        .unwrap();

    assert_eq!(answer.probability(), 0.92);
}

#[tokio::test]
async fn is_likely_at_one_half_or_more() {
    for (probability, likely) in [(0.5, true), (0.49, false)] {
        let server = mock_api(noul_response(probability)).await;

        let answer = client_for(&server)
            .ask(Noul::new(SPAM).unwrap(), MESSAGE)
            .await
            .unwrap();

        assert_eq!(answer.is_likely(), likely, "{probability}");
    }
}

#[tokio::test]
async fn the_request_has_the_shape_of_the_api() {
    let server = mock_api(noul_response(0.92)).await;

    client_for(&server)
        .ask(Noul::new(SPAM).unwrap(), MESSAGE)
        .await
        .unwrap();

    let request = only_request(&server).await;
    let header = |name| request.headers[name].to_str().unwrap();
    assert_eq!(header("authorization"), "Bearer key");
    assert_eq!(header("content-type"), "application/json");
    assert_eq!(header("content-encoding"), "zstd");
    assert_eq!(
        request_json(&request),
        json!({
            "model": "jev-latest",
            "state": MESSAGE,
            "questions": { "q0": { "type": "noul", "instructions": SPAM } },
        })
    );
}

#[tokio::test]
async fn a_noul_sends_only_the_criteria_that_it_has() {
    let server = mock_api(noul_response(0.92)).await;
    let question = Noul::with_criteria(SPAM, json!({ "true": "It sells a product." })).unwrap();

    client_for(&server).ask(question, MESSAGE).await.unwrap();

    assert_eq!(
        request_json(&only_request(&server).await)["questions"]["q0"],
        json!({
            "type": "noul",
            "instructions": SPAM,
            "criteria": { "true": "It sells a product." },
        })
    );
}

#[tokio::test]
async fn model_names_the_model_of_the_request() {
    let server = mock_api(noul_response(0.92)).await;

    client_for(&server)
        .model("jev-1.14.0")
        .ask(Noul::new(SPAM).unwrap(), MESSAGE)
        .await
        .unwrap();

    assert_eq!(
        request_json(&only_request(&server).await)["model"],
        "jev-1.14.0"
    );
}

#[tokio::test]
async fn base_url_keeps_its_path() {
    let server = MockServer::start().await;
    Mock::given(path("/proxy/v1/systemone"))
        .respond_with(noul_response(0.92))
        .mount(&server)
        .await;

    let answer = Client::new()
        .api_key("key")
        .base_url(format!("{}/proxy/", server.uri()))
        .ask(Noul::new(SPAM).unwrap(), MESSAGE)
        .await;

    assert!(answer.is_ok());
}

#[tokio::test]
async fn ask_refuses_a_bad_base_url() {
    for url in ["api.typesafe.ai", "ftp://api.typesafe.ai", "localhost:8080"] {
        let error = Client::new()
            .api_key("key")
            .base_url(url)
            .ask(Noul::new(SPAM).unwrap(), MESSAGE)
            .await
            .unwrap_err();

        assert!(matches!(error, AskError::InvalidBaseUrl { .. }), "{url}");
        assert_eq!(
            error.to_string(),
            format!("the base URL must be an absolute HTTP or HTTPS URL, not `{url}`")
        );
    }
}

#[tokio::test]
async fn ask_needs_an_api_key() {
    let server = mock_api(noul_response(0.92)).await;

    for client in [Client::new(), Client::new().api_key("")] {
        let error = client
            .base_url(server.uri())
            .ask(Noul::new(SPAM).unwrap(), MESSAGE)
            .await
            .unwrap_err();

        assert!(matches!(error, AskError::MissingApiKey));
        assert_eq!(
            error.to_string(),
            "a request must have an API key: set `TYPESAFE_API_KEY`, or call `Client::api_key`"
        );
    }
    assert!(no_request_sent(&server).await);
}

#[tokio::test]
async fn ask_refuses_a_bad_state_before_a_request() {
    let server = mock_api(noul_response(0.92)).await;

    let error = client_for(&server)
        .ask(Noul::new(SPAM).unwrap(), "")
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::State(InvalidJson::Empty)));
    assert_eq!(error.to_string(), "the state must not be empty");
    assert!(no_request_sent(&server).await);
}

#[tokio::test]
async fn ask_stops_at_the_timeout() {
    let server = mock_api(noul_response(0.92).set_delay(Duration::from_millis(500))).await;

    let error = client_for(&server)
        .timeout(Duration::from_millis(50))
        .ask(Noul::new(SPAM).unwrap(), MESSAGE)
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::Timeout));
    assert_eq!(
        error.to_string(),
        "the response must arrive within the timeout: raise it with `Client::timeout`"
    );
}

#[tokio::test]
async fn ask_reports_a_failed_connection() {
    let closed = {
        let server = MockServer::start().await;
        server.uri()
    };

    let error = Client::new()
        .api_key("key")
        .base_url(&closed)
        .ask(Noul::new(SPAM).unwrap(), MESSAGE)
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::Transport(_)));
    assert!(error.source().is_some());
    assert_eq!(
        error.to_string(),
        format!(
            "the connection to the API failed: error sending request for url ({closed}/v1/systemone)"
        )
    );
}

#[tokio::test]
async fn ask_reports_a_refusal_of_the_api() {
    let server = mock_api(ResponseTemplate::new(401).set_body_string("Unauthorized")).await;

    let error = client_for(&server)
        .ask(Noul::new(SPAM).unwrap(), MESSAGE)
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::Http { status: 401, .. }));
    assert_eq!(
        error.to_string(),
        "the API refused the request with status 401: Unauthorized"
    );
}

#[tokio::test]
async fn ask_refuses_a_response_that_is_not_json() {
    let server = mock_api(ResponseTemplate::new(200).set_body_string("<html>")).await;

    let error = client_for(&server)
        .ask(Noul::new(SPAM).unwrap(), MESSAGE)
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::InvalidResponse { .. }));
    assert_eq!(
        error.to_string(),
        "the response must be JSON: expected value at line 1 column 1"
    );
}

#[tokio::test]
async fn ask_refuses_a_response_with_the_wrong_answer() {
    let server = mock_api(response_with(&json!({ "type": "box", "confidence": 0.9 }))).await;

    let error = client_for(&server)
        .ask(Noul::new(SPAM).unwrap(), MESSAGE)
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::InvalidResponse { .. }));
    assert_eq!(
        error.to_string(),
        "the response must answer each question: unknown variant `box`, expected `noul`"
    );
}

#[test]
fn debug_shows_the_settings_and_hides_the_key() {
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
