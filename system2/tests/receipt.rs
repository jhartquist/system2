mod support;

use serde_json::json;
use std::time::Duration;
use support::{
    INSTRUCTIONS, STATE, answer_set_body, answer_set_response, cache_path, client_for, mock_api,
    mock_api_then, noul, noul_answer, noul_response, request_count,
};
use system2::prelude::*;
use system2::{AskError, Client, Noul, Usage};
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};
use wiremock::ResponseTemplate;

const REQUEST_ID: &str = "req_01a13d1fbca4728f8f7c1cdd312bd3a5";

fn tokens(usage: Usage) -> (u64, u64) {
    (usage.input_tokens, usage.output_tokens)
}

#[tokio::test]
async fn ask_with_receipt_tells_the_model_the_tokens_and_the_request_id() {
    let answers = answer_set_response(&[&noul_answer(0.1), &noul_answer(0.9)])
        .insert_header("x-typesafe-request-id", REQUEST_ID)
        .set_delay(Duration::from_millis(200));
    let server = mock_api(answers).await;

    // spawned, to show that the future is `Send`
    let asking =
        client_for(&server).ask_with_receipt((noul("Is it spam?"), noul("Is it urgent?")), STATE);
    let ((spam, urgent), receipt) = tokio::spawn(asking).await.unwrap().unwrap();

    assert_eq!((spam.probability(), urgent.probability()), (0.1, 0.9));
    assert_eq!(receipt.model.as_deref(), Some("jev-1.13.0"));
    assert_eq!(tokens(receipt.usage), (280, 1));
    assert!(!receipt.cached);
    assert!(receipt.latency >= Duration::from_millis(200), "{receipt:?}");
    assert_eq!(receipt.request_id.as_deref(), Some(REQUEST_ID));
}

#[tokio::test]
async fn ask_with_receipt_counts_the_retries_and_their_waits_in_the_latency() {
    let retry = ResponseTemplate::new(503).insert_header("retry-after-ms", "300");
    let server = mock_api_then(retry, noul_response(0.92)).await;

    let (_, receipt) = client_for(&server)
        .ask_with_receipt(noul(INSTRUCTIONS), STATE)
        .await
        .unwrap();

    assert!(receipt.latency >= Duration::from_millis(300), "{receipt:?}");
}

#[tokio::test]
async fn ask_with_receipt_counts_no_wait_for_the_limit_in_the_latency() {
    let server = mock_api(noul_response(0.92).set_delay(Duration::from_millis(500))).await;
    let client = client_for(&server).max_concurrent_requests(1);

    let mut asks = JoinSet::new();
    for _ in 0..2 {
        asks.spawn(client.ask_with_receipt(noul(INSTRUCTIONS), STATE));
    }
    let receipts = asks.join_all().await;

    // the second request waits 500 ms for the first, and its latency must not count that wait
    for receipt in receipts {
        let (_, receipt) = receipt.unwrap();
        assert!(receipt.latency < Duration::from_millis(900), "{receipt:?}");
    }
}

#[tokio::test]
async fn ask_with_receipt_tells_an_answer_from_the_cache() {
    let path = cache_path("ask_with_receipt_tells_an_answer_from_the_cache");
    let answer = noul_response(0.92).insert_header("x-typesafe-request-id", REQUEST_ID);
    let server = mock_api(answer).await;
    let client = client_for(&server).cache(&path).unwrap();

    let (_, sent) = client
        .ask_with_receipt(noul(INSTRUCTIONS), STATE)
        .await
        .unwrap();
    let (_, cached) = client
        .ask_with_receipt(noul(INSTRUCTIONS), STATE)
        .await
        .unwrap();

    assert!(!sent.cached);
    assert!(cached.cached);
    assert_eq!(cached.model.as_deref(), Some("jev-1.13.0"));
    assert_eq!(tokens(cached.usage), (0, 0));
    assert_eq!(cached.latency, Duration::ZERO);
    assert_eq!(cached.request_id, None);
    assert_eq!(client.usage(), sent.usage);
    assert_eq!(request_count(&server).await, 1);
}

#[tokio::test]
async fn ask_with_receipt_tells_nothing_for_a_set_with_no_questions() {
    let client = Client::new();

    let (answers, receipt) = client
        .ask_with_receipt(Vec::<Noul>::new(), STATE)
        .await
        .unwrap();

    assert!(answers.is_empty());
    assert_eq!(receipt.model, None);
    assert_eq!(tokens(receipt.usage), (0, 0));
    assert!(!receipt.cached);
    assert_eq!(receipt.latency, Duration::ZERO);
    assert_eq!(receipt.request_id, None);
    assert_eq!(tokens(client.usage()), (0, 0));
}

#[tokio::test]
async fn ask_with_receipt_counts_no_usage_as_zero_tokens() {
    let mut body = answer_set_body(&[&noul_answer(0.92)]);
    body.as_object_mut().unwrap().remove("usage");
    let server = mock_api(ResponseTemplate::new(200).set_body_json(body)).await;
    let client = client_for(&server);

    let (_, receipt) = client
        .ask_with_receipt(noul(INSTRUCTIONS), STATE)
        .await
        .unwrap();

    assert_eq!(tokens(receipt.usage), (0, 0));
    assert_eq!(tokens(client.usage()), (0, 0));
}

#[tokio::test]
async fn ask_with_receipt_tells_no_model_for_a_response_with_none() {
    let mut body = answer_set_body(&[&noul_answer(0.92)]);
    body.as_object_mut().unwrap().remove("model");
    let server = mock_api(ResponseTemplate::new(200).set_body_json(body)).await;

    let (_, receipt) = client_for(&server)
        .ask_with_receipt(noul(INSTRUCTIONS), STATE)
        .await
        .unwrap();

    assert_eq!(receipt.model, None);
}

#[tokio::test]
async fn ask_refuses_a_model_or_tokens_of_the_wrong_type() {
    let cases = [
        (
            "model",
            json!(13),
            "the response must have the shape of the API: \
             invalid type: integer `13`, expected a string",
        ),
        (
            "usage",
            json!([280, 1]),
            "the response must give `usage` as an object of token counts, not `[280,1]`",
        ),
    ];
    for (key, value, message) in cases {
        let mut body = answer_set_body(&[&noul_answer(0.92)]);
        body[key] = value;
        let server = mock_api(ResponseTemplate::new(200).set_body_json(body)).await;

        let error = client_for(&server)
            .ask(noul(INSTRUCTIONS), STATE)
            .await
            .unwrap_err();

        assert!(
            matches!(error, AskError::InvalidResponse { .. }),
            "{error:?}"
        );
        assert_eq!(error.to_string(), message);
    }
}

#[tokio::test]
async fn usage_totals_the_tokens_of_a_client_and_its_clones() {
    let server = mock_api(noul_response(0.92)).await;
    let client = client_for(&server);
    let clone = client
        .clone()
        .model("jev-1.14.0")
        .max_concurrent_requests(1);

    client.ask(noul(INSTRUCTIONS), STATE).await.unwrap();
    clone.ask(noul(INSTRUCTIONS), STATE).await.unwrap();
    client.clone().ask(noul(INSTRUCTIONS), STATE).await.unwrap();

    assert_eq!(tokens(client.usage()), (840, 3));
    assert_eq!(clone.usage(), client.usage());
    assert_eq!(tokens(client_for(&server).usage()), (0, 0));
}

#[tokio::test]
async fn usage_counts_the_tokens_of_a_response_that_fails() {
    let mut no_answers = answer_set_body(&[]);
    no_answers.as_object_mut().unwrap().remove("answers");
    let mut bad_model = answer_set_body(&[&noul_answer(0.92)]);
    bad_model["model"] = json!(13);
    let cases = [
        answer_set_body(&[&json!({ "type": "noul", "noul": 1.5 })]),
        no_answers,
        bad_model,
    ];
    for body in cases {
        let server = mock_api(ResponseTemplate::new(200).set_body_json(&body)).await;
        let client = client_for(&server);

        client.ask(noul(INSTRUCTIONS), STATE).await.unwrap_err();

        assert_eq!(tokens(client.usage()), (280, 1), "{body}");
    }
}

#[tokio::test]
async fn usage_saturates_at_the_most_tokens() {
    let mut body = answer_set_body(&[&noul_answer(0.92)]);
    body["usage"] = json!({ "input_tokens": u64::MAX, "output_tokens": u64::MAX });
    let server = mock_api(ResponseTemplate::new(200).set_body_json(body)).await;
    let client = client_for(&server);

    for _ in 0..2 {
        client.ask(noul(INSTRUCTIONS), STATE).await.unwrap();
    }

    assert_eq!(tokens(client.usage()), (u64::MAX, u64::MAX));
}

#[tokio::test]
async fn usage_adds_nothing_for_a_dropped_request() {
    let server = mock_api(noul_response(0.92).set_delay(Duration::from_secs(1))).await;
    let client = client_for(&server);

    // wide margins, as in the test of the cache
    timeout(
        Duration::from_millis(300),
        client.ask(noul(INSTRUCTIONS), STATE),
    )
    .await
    .unwrap_err();
    sleep(Duration::from_secs(1)).await;

    assert_eq!(request_count(&server).await, 1);
    assert_eq!(tokens(client.usage()), (0, 0));
}

// the only test here that uses the global client, so no other test can change it
#[tokio::test]
async fn ask_with_receipt_uses_the_global_client() {
    let server = mock_api(answer_set_response(&[&noul_answer(0.1), &noul_answer(0.9)])).await;
    let client = client_for(&server).set_global();
    let texts = ["spam", "urgent"];

    let ((spam, urgent), set_receipt) = (noul("Is it spam?"), noul("Is it urgent?"))
        .ask_with_receipt(STATE)
        .await
        .unwrap();
    let (answers, iterator_receipt) = texts
        .iter()
        .map(|text| noul(&format!("Is it {text}?")))
        .ask_with_receipt(STATE)
        .await
        .unwrap();

    assert_eq!((spam.probability(), urgent.probability()), (0.1, 0.9));
    assert_eq!(answers.len(), 2);
    assert_eq!(tokens(set_receipt.usage), (280, 1));
    assert_eq!(tokens(iterator_receipt.usage), (280, 1));
    assert_eq!(tokens(client.usage()), (560, 2));
}
