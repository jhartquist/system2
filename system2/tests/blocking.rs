//! Tests of `system2::blocking` against a mock server.

mod support;

use std::time::Duration;
use support::{
    AnswerByType, INSTRUCTIONS, STATE, answer_set_response, block_on, client_for, mock_api, mount,
    noul, noul_answer, noul_response, request_count,
};
use system2::blocking::prelude::*;
use system2::{AskError, Client, blocking};
use tokio::runtime::Runtime;
use wiremock::MockServer;

// the only test here that uses the global client, as it is one for each process
#[test]
fn ask_uses_the_global_client_for_each_question_set() {
    let server = block_on(MockServer::start());
    block_on(mount(&server, AnswerByType));
    let global = client_for(&server).set_global();

    let spam = noul(INSTRUCTIONS).ask(STATE).unwrap();
    let ((urgent, rude), tag) = (noul("Is it urgent?"), noul("Is it rude?"))
        .tag("ticket")
        .ask(STATE)
        .unwrap();
    let (_, receipt) = noul(INSTRUCTIONS).ask_with_receipt(STATE).unwrap();
    let checks = ["a", "b"].iter().map(|text| noul(text)).ask(STATE).unwrap();
    let (more_checks, _) = ["c"]
        .iter()
        .map(|text| noul(text))
        .ask_with_receipt(STATE)
        .unwrap();

    assert_eq!(spam.probability(), 0.95);
    assert_eq!((urgent.probability(), rude.probability()), (0.95, 0.95));
    assert_eq!(tag, "ticket");
    assert_eq!(receipt.usage.input_tokens, 280);
    assert_eq!(checks.len(), 2);
    assert_eq!(more_checks.len(), 1);
    assert_eq!(block_on(request_count(&server)), 5);
    assert_eq!(global.usage().input_tokens, 5 * 280);
}

#[test]
fn ask_returns_the_answers_of_the_client() {
    let answers = answer_set_response(&[&noul_answer(0.1), &noul_answer(0.9)]);
    let server = block_on(mock_api(answers));
    let client = blocking::Client::from(client_for(&server));

    let (spam, urgent) = client
        .ask((noul("Is it spam?"), noul("Is it urgent?")), STATE)
        .unwrap();

    assert_eq!((spam.probability(), urgent.probability()), (0.1, 0.9));
}

#[test]
fn ask_returns_the_error_of_the_client() {
    let client = blocking::Client::from(Client::new().offline(true));

    let error = client.ask(noul(INSTRUCTIONS), STATE).unwrap_err();

    assert!(matches!(error, AskError::Offline), "{error:?}");
}

#[test]
fn ask_with_receipt_returns_the_answers_and_the_receipt() {
    let server = block_on(mock_api(
        noul_response(0.92).insert_header("x-typesafe-request-id", "req_1"),
    ));
    let client = blocking::Client::from(client_for(&server));

    let (answer, receipt) = client.ask_with_receipt(noul(INSTRUCTIONS), STATE).unwrap();

    assert_eq!(answer.probability(), 0.92);
    assert_eq!(receipt.usage.input_tokens, 280);
    assert_eq!(receipt.request_id.as_deref(), Some("req_1"));
}

#[test]
fn usage_counts_the_tokens_of_the_client_its_clones_and_its_source() {
    let server = block_on(mock_api(noul_response(0.92)));
    let source = client_for(&server);
    let client = blocking::Client::from(source.clone());

    client.ask(noul(INSTRUCTIONS), STATE).unwrap();
    client.clone().ask(noul(INSTRUCTIONS), STATE).unwrap();

    assert_eq!(client.usage().input_tokens, 2 * 280);
    assert_eq!(client.usage(), source.usage());
}

// a connection runs on the runtime that opened it, so that runtime must run while it is idle
#[test]
fn ask_uses_a_connection_that_another_client_opened() {
    let server = block_on(mock_api(noul_response(0.92)));
    let source = client_for(&server)
        .timeout(Duration::from_secs(2))
        .max_retries(0);
    let first = blocking::Client::from(source.clone());
    let second = blocking::Client::from(source.clone());

    first.ask(noul(INSTRUCTIONS), STATE).unwrap();
    let answer = second.ask(noul(INSTRUCTIONS), STATE).unwrap();

    assert_eq!(answer.probability(), 0.92);
}

#[test]
fn ask_leaves_its_connections_to_async_clients() {
    let server = block_on(mock_api(noul_response(0.92)));
    let source = client_for(&server)
        .timeout(Duration::from_secs(2))
        .max_retries(0);
    let client = blocking::Client::from(source.clone());

    client.ask(noul(INSTRUCTIONS), STATE).unwrap();
    let answer = Runtime::new()
        .unwrap()
        .block_on(source.ask(noul(INSTRUCTIONS), STATE))
        .unwrap();

    assert_eq!(answer.probability(), 0.92);
}

#[tokio::test]
#[should_panic(expected = "Cannot start a runtime from within a runtime")]
async fn ask_panics_in_an_async_runtime() {
    let client = blocking::Client::from(Client::new().offline(true));

    let _ = client.ask(noul(INSTRUCTIONS), STATE);
}

#[tokio::test]
async fn drop_stops_the_runtime_in_an_async_runtime() {
    let server = mock_api(noul_response(0.92)).await;
    let client = blocking::Client::from(client_for(&server));
    let worker = client.clone();

    let answer = tokio::task::spawn_blocking(move || worker.ask(noul(INSTRUCTIONS), STATE))
        .await
        .unwrap()
        .unwrap();
    drop(client);

    assert_eq!(answer.probability(), 0.92);
}
