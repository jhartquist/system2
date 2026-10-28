mod support;

use serde_json::json;
use std::collections::BTreeMap;
use std::error::Error;
use std::net::TcpListener;
use std::time::Duration;
use support::{
    INSTRUCTIONS, STATE, client_for, mock_api, no_request_sent, noul_response, only_request,
    request_body,
};
use system2::{AskError, Client, InvalidJson, Noul};
use tokio::io::AsyncReadExt;
use tokio::time::timeout;
use wiremock::ResponseTemplate;

const REQUEST_ID: &str = "req_01a13d1fbca4728f8f7c1cdd312bd3a5";

#[tokio::test]
async fn ask_sends_a_noul_and_reads_its_answer() {
    let server = mock_api(noul_response(0.92)).await;
    let question =
        Noul::with_criteria(INSTRUCTIONS, json!({ "true": "It sells a product." })).unwrap();

    let answer = client_for(&server)
        .model("jev-1.14.0")
        .ask(question, STATE)
        .await
        .unwrap();

    assert_eq!(answer.probability(), 0.92);
    let request = only_request(&server).await;
    let header = |name| request.headers[name].to_str().unwrap();
    assert_eq!(header("authorization"), "Bearer key");
    assert_eq!(header("content-type"), "application/json");
    assert_eq!(header("content-encoding"), "zstd");
    assert_eq!(
        request_body(&request),
        json!({
            "model": "jev-1.14.0",
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
async fn ask_returns_a_future_that_can_be_spawned() {
    let server = mock_api(noul_response(0.92)).await;
    let state = String::from(STATE);

    let asking = client_for(&server).ask(Noul::new(INSTRUCTIONS).unwrap(), &state);
    drop(state);
    let answer = tokio::spawn(asking).await.unwrap().unwrap();

    assert_eq!(answer.probability(), 0.92);
}

#[tokio::test]
async fn ask_cancels_the_request_when_dropped() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = Client::new()
        .api_key("key")
        .base_url(format!("http://{}", listener.local_addr().unwrap()));

    let received = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket.read_exact(&mut [0]).await.unwrap();
        socket
    };

    let mut asking = client.ask(Noul::new(INSTRUCTIONS).unwrap(), STATE);
    let mut socket = tokio::select! {
        _ = &mut asking => panic!("the server never answers"),
        socket = timeout(Duration::from_secs(5), received) => {
            socket.expect("the server must receive the request")
        },
    };

    drop(asking);

    timeout(Duration::from_secs(5), socket.read_to_end(&mut Vec::new()))
        .await
        .expect("the client must close the connection")
        .unwrap();
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
async fn ask_tells_why_the_state_does_not_serialize() {
    let server = mock_api(noul_response(0.92)).await;
    let state = BTreeMap::from([((1, 2), "a pair as a key")]);

    let error = client_for(&server)
        .ask(Noul::new(INSTRUCTIONS).unwrap(), &state)
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::State(InvalidJson::Serialize(_))));
    assert_eq!(
        error.to_string(),
        "the state must serialize to JSON: key must be a string"
    );
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
            "the connection to the API failed: \
             error sending request for url ({closed}/v1/systemone)"
        )
    );
}

#[tokio::test]
async fn ask_reports_an_error_status() {
    let body = json!({
        "detail": { "error_type": "api_usage_error", "message": "Unknown model: jev-1.14" },
    });
    let response = ResponseTemplate::new(400)
        .insert_header("x-typesafe-request-id", REQUEST_ID)
        .set_body_json(body);
    let server = mock_api(response).await;

    let error = client_for(&server)
        .ask(Noul::new(INSTRUCTIONS).unwrap(), STATE)
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        format!(
            "the API refused the request with status 400: \
             Unknown model: jev-1.14 (request ID `{REQUEST_ID}`)"
        )
    );
    let AskError::Http {
        status, request_id, ..
    } = error
    else {
        panic!("expected an HTTP error, got {error:?}");
    };
    assert_eq!(status, 400);
    assert_eq!(request_id.as_deref(), Some(REQUEST_ID));
}

#[tokio::test]
async fn ask_reports_the_token_limit() {
    let body = json!({ "detail": { "error_type": "max_tokens_exceeded" } });
    let server = mock_api(ResponseTemplate::new(400).set_body_json(body)).await;

    let error = client_for(&server)
        .ask(Noul::new(INSTRUCTIONS).unwrap(), STATE)
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::TokenLimit));
    assert_eq!(
        error.to_string(),
        "the request has too many tokens: use `Client::long_context(true)`"
    );
}

#[tokio::test]
async fn ask_reports_an_invalid_response() {
    let response = ResponseTemplate::new(200)
        .insert_header("x-typesafe-request-id", REQUEST_ID)
        .set_body_string("<html>");
    let server = mock_api(response).await;

    let error = client_for(&server)
        .ask(Noul::new(INSTRUCTIONS).unwrap(), STATE)
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        format!(
            "the response must be JSON: \
             expected value at line 1 column 1 (request ID `{REQUEST_ID}`)"
        )
    );
    let AskError::InvalidResponse { request_id, .. } = error else {
        panic!("expected an invalid response, got {error:?}");
    };
    assert_eq!(request_id.as_deref(), Some(REQUEST_ID));
}
