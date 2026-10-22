use serde_json::{Value, json};
use std::error::Error;
use std::net::TcpListener;
use std::time::Duration;
use system2::{AskError, Client, InvalidJson, Noul};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const INSTRUCTIONS: &str = "Is this message spam?";
const STATE: &str = "Buy now!";

async fn mock_api(response: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

fn noul_response(probability: f64) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "model": "jev-1.13.0",
        "answers": { "q0": { "type": "noul", "noul": probability } },
        "usage": { "input_tokens": 280, "output_tokens": 1 },
    }))
}

fn client_for(server: &MockServer) -> Client {
    Client::new().api_key("key").base_url(server.uri())
}

async fn no_request_sent(server: &MockServer) -> bool {
    server.received_requests().await.unwrap().is_empty()
}

#[tokio::test]
async fn ask_sends_a_noul_and_reads_its_answer() {
    let server = mock_api(noul_response(0.92)).await;
    let question =
        Noul::with_criteria(INSTRUCTIONS, json!({ "true": "It sells a product." })).unwrap();

    let answer = client_for(&server).ask(question, STATE).await.unwrap();

    assert!(answer.is_likely());
    let requests = server.received_requests().await.unwrap();
    let [request] = requests.as_slice() else {
        panic!("{} requests", requests.len());
    };
    let header = |name| request.headers[name].to_str().unwrap();
    assert_eq!(header("authorization"), "Bearer key");
    assert_eq!(header("content-type"), "application/json");
    assert_eq!(header("content-encoding"), "zstd");
    let body = zstd::decode_all(request.body.as_slice()).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({
            "model": "jev-latest",
            "state": STATE,
            "questions": {
                "q0": {
                    "type": "noul",
                    "instructions": INSTRUCTIONS,
                    "criteria": { "true": "It sells a product." },
                },
            },
        })
    );
}

#[tokio::test]
async fn ask_refuses_a_bad_state_before_a_request() {
    let server = mock_api(noul_response(0.92)).await;

    let error = client_for(&server)
        .ask(Noul::new(INSTRUCTIONS).unwrap(), "")
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::State(InvalidJson::Empty)));
    assert_eq!(error.to_string(), "the state must not be empty");
    assert!(no_request_sent(&server).await);
}

#[tokio::test]
async fn ask_refuses_a_missing_or_empty_api_key() {
    let server = mock_api(noul_response(0.92)).await;

    for client in [Client::new(), Client::new().api_key("")] {
        let error = client
            .base_url(server.uri())
            .ask(Noul::new(INSTRUCTIONS).unwrap(), STATE)
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
async fn ask_stops_at_the_timeout() {
    let server = mock_api(noul_response(0.92).set_delay(Duration::from_millis(500))).await;

    let error = client_for(&server)
        .timeout(Duration::from_millis(50))
        .ask(Noul::new(INSTRUCTIONS).unwrap(), STATE)
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
    // wiremock reuses a dropped server, so a closed port comes from a listener
    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    };

    let error = Client::new()
        .api_key("key")
        .base_url(&closed)
        .ask(Noul::new(INSTRUCTIONS).unwrap(), STATE)
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
async fn ask_reports_an_error_status() {
    let server = mock_api(ResponseTemplate::new(401).set_body_string("Unauthorized")).await;

    let error = client_for(&server)
        .ask(Noul::new(INSTRUCTIONS).unwrap(), STATE)
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::Http { status: 401, .. }));
    assert_eq!(
        error.to_string(),
        "the API refused the request with status 401: Unauthorized"
    );
}
