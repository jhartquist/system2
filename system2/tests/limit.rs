//! Tests of the limit on requests at the same time.

mod support;

use std::time::Duration;
use support::{
    INSTRUCTIONS, STATE, client_for, mock_api, mock_api_then, noul, noul_response, request_count,
};
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};
use wiremock::{MockServer, ResponseTemplate};

// wide margins, so a busy machine sends each request in the limit before the count, and a
// request over the limit cannot get its place before it
const ANSWER_DELAY: Duration = Duration::from_secs(1);
const COUNT_DELAY: Duration = Duration::from_millis(500);

async fn slow_api() -> MockServer {
    mock_api(noul_response(0.92).set_delay(ANSWER_DELAY)).await
}

#[tokio::test]
async fn ask_sends_no_more_than_max_concurrent_requests_at_once() {
    for (max_concurrent_requests, limit) in [(Some(2), 2), (None, 8)] {
        let server = slow_api().await;
        let client = client_for(&server);
        let client = match max_concurrent_requests {
            Some(max_concurrent_requests) => {
                client.max_concurrent_requests(max_concurrent_requests)
            }
            None => client,
        };

        let mut asks = JoinSet::new();
        for _ in 0..=limit {
            asks.spawn(client.ask(noul(INSTRUCTIONS), STATE));
        }
        sleep(COUNT_DELAY).await;

        assert_eq!(
            request_count(&server).await,
            limit,
            "{max_concurrent_requests:?}"
        );
        let answers = timeout(ANSWER_DELAY * 3, asks.join_all())
            .await
            .expect("each request must get its place in the limit");
        for answer in answers {
            answer.unwrap();
        }
        assert_eq!(
            request_count(&server).await,
            limit + 1,
            "{max_concurrent_requests:?}"
        );
    }
}

#[tokio::test]
async fn ask_shares_the_limit_with_each_clone() {
    let server = slow_api().await;
    let client = client_for(&server).max_concurrent_requests(1);

    let mut asks = JoinSet::new();
    asks.spawn(client.ask(noul(INSTRUCTIONS), STATE));
    asks.spawn(client.clone().ask(noul(INSTRUCTIONS), STATE));
    asks.spawn(
        client
            .clone()
            .model("jev-1.14.0")
            .ask(noul(INSTRUCTIONS), STATE),
    );
    sleep(COUNT_DELAY).await;

    assert_eq!(request_count(&server).await, 1);
}

#[tokio::test]
async fn max_concurrent_requests_gives_a_clone_a_limit_of_its_own() {
    let server = slow_api().await;
    let client = client_for(&server).max_concurrent_requests(1);

    let mut asks = JoinSet::new();
    asks.spawn(client.ask(noul(INSTRUCTIONS), STATE));
    asks.spawn(
        client
            .clone()
            .max_concurrent_requests(1)
            .ask(noul(INSTRUCTIONS), STATE),
    );
    sleep(COUNT_DELAY).await;

    assert_eq!(request_count(&server).await, 2);
}

#[tokio::test]
async fn ask_keeps_its_place_in_the_limit_through_its_retries() {
    let first = ResponseTemplate::new(503).insert_header("retry-after-ms", "1000");
    let server = mock_api_then(first, noul_response(0.92)).await;
    let client = client_for(&server).max_concurrent_requests(1);

    let mut asks = JoinSet::new();
    asks.spawn(client.ask(noul(INSTRUCTIONS), STATE));
    asks.spawn(client.ask(noul(INSTRUCTIONS), STATE));
    sleep(COUNT_DELAY).await;

    assert_eq!(request_count(&server).await, 1);
}

#[tokio::test]
async fn ask_gives_back_its_place_in_the_limit_when_dropped() {
    let server = slow_api().await;
    let client = client_for(&server).max_concurrent_requests(1);

    for _ in 0..2 {
        timeout(COUNT_DELAY, client.ask(noul(INSTRUCTIONS), STATE))
            .await
            .expect_err("the answer must take longer than the timeout");
    }

    assert_eq!(request_count(&server).await, 2);
}
