mod support;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime};
use support::{
    INSTRUCTIONS, STATE, client_for, mock_api, noul, noul_response, partial_api, request_count,
};
use system2::AskError;
use tokio::time::{sleep, timeout};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// the first wait of the backoff, less the most that jitter subtracts
const SHORTEST_BACKOFF: Duration = Duration::from_millis(375);

async fn mock_api_then(first: ResponseTemplate, then: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(first)
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(then)
        .mount(&server)
        .await;
    server
}

fn retry_now(status: u16) -> ResponseTemplate {
    ResponseTemplate::new(status).insert_header("retry-after-ms", "0")
}

#[tokio::test]
async fn ask_retries_408_429_and_5xx() {
    for status in [408, 429, 500, 503, 599] {
        let server = mock_api_then(retry_now(status), noul_response(0.92)).await;

        let answer = client_for(&server)
            .ask(noul(INSTRUCTIONS), STATE)
            .await
            .unwrap();

        assert_eq!(answer.probability(), 0.92, "{status}");
        assert_eq!(request_count(&server).await, 2, "{status}");
    }
}

#[tokio::test]
async fn ask_returns_other_statuses_with_no_retry() {
    for status in [400, 401, 402, 404, 413, 422] {
        let server = mock_api(retry_now(status)).await;

        let error = client_for(&server)
            .ask(noul(INSTRUCTIONS), STATE)
            .await
            .unwrap_err();

        assert!(
            matches!(error, AskError::Http { status: s, .. } if s == status),
            "{status}: {error:?}"
        );
        assert_eq!(request_count(&server).await, 1, "{status}");
    }
}

#[tokio::test]
async fn ask_retries_as_many_times_as_max_retries() {
    for (max_retries, attempts) in [(None, 3), (Some(0), 1), (Some(1), 2), (Some(4), 5)] {
        let server = mock_api(retry_now(503)).await;
        let client = client_for(&server);
        let client = match max_retries {
            Some(max_retries) => client.max_retries(max_retries),
            None => client,
        };

        let error = client.ask(noul(INSTRUCTIONS), STATE).await.unwrap_err();

        assert!(
            matches!(error, AskError::Http { status: 503, .. }),
            "{max_retries:?}: {error:?}"
        );
        assert_eq!(request_count(&server).await, attempts, "{max_retries:?}");
    }
}

#[tokio::test]
async fn ask_waits_as_a_header_tells() {
    // each wait is longer than the longest first backoff
    let cases = [
        ("retry-after-ms", "600", Duration::from_millis(600)),
        ("retry-after", "1", Duration::from_secs(1)),
    ];
    for (header, value, wait) in cases {
        let first = ResponseTemplate::new(429).insert_header(header, value);
        let server = mock_api_then(first, noul_response(0.92)).await;

        let start = Instant::now();
        client_for(&server)
            .ask(noul(INSTRUCTIONS), STATE)
            .await
            .unwrap();

        assert!(start.elapsed() >= wait, "{header}: {:?}", start.elapsed());
    }
}

#[tokio::test]
async fn ask_waits_for_the_backoff_with_no_header() {
    let server = mock_api_then(ResponseTemplate::new(503), noul_response(0.92)).await;

    let start = Instant::now();
    client_for(&server)
        .ask(noul(INSTRUCTIONS), STATE)
        .await
        .unwrap();

    assert!(start.elapsed() >= SHORTEST_BACKOFF, "{:?}", start.elapsed());
}

#[tokio::test]
async fn ask_returns_the_error_when_a_header_asks_for_more_than_30_seconds() {
    let in_an_hour = httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(3600));
    let cases = [
        ("retry-after-ms", "30001".to_owned()),
        ("retry-after", "31".to_owned()),
        ("retry-after", in_an_hour),
    ];
    for (header, value) in cases {
        let first = ResponseTemplate::new(429).insert_header(header, value.as_str());
        let server = mock_api_then(first, noul_response(0.92)).await;

        let error = client_for(&server)
            .ask(noul(INSTRUCTIONS), STATE)
            .await
            .unwrap_err();

        assert!(
            matches!(error, AskError::Http { status: 429, .. }),
            "{value}: {error:?}"
        );
        assert_eq!(request_count(&server).await, 1, "{value}");
    }
}

#[tokio::test]
async fn ask_follows_no_redirect() {
    for status in [301, 307] {
        let response = ResponseTemplate::new(status).insert_header("location", "/v1/systemone");
        let server = mock_api(response).await;

        let error = client_for(&server)
            .ask(noul(INSTRUCTIONS), STATE)
            .await
            .unwrap_err();

        assert!(
            matches!(error, AskError::Http { status: s, .. } if s == status),
            "{status}: {error:?}"
        );
        assert_eq!(request_count(&server).await, 1, "{status}");
    }
}

#[tokio::test]
async fn ask_retries_a_timeout() {
    let slow = noul_response(0.1).set_delay(Duration::from_secs(5));
    let server = mock_api_then(slow, noul_response(0.92)).await;

    let answer = client_for(&server)
        .timeout(Duration::from_millis(500))
        .ask(noul(INSTRUCTIONS), STATE)
        .await
        .unwrap();

    assert_eq!(answer.probability(), 0.92);
    assert_eq!(request_count(&server).await, 2);
}

#[tokio::test]
async fn ask_retries_a_failed_connection() {
    // the connection closes before the response, or in its body
    let cases: [&[u8]; 2] = [b"", b"HTTP/1.1 200 OK\r\ncontent-length: 100\r\n\r\n{"];
    for partial_response in cases {
        let (client, connections) = partial_api(partial_response).await;

        let error = client
            .max_retries(1)
            .ask(noul(INSTRUCTIONS), STATE)
            .await
            .unwrap_err();

        assert!(matches!(error, AskError::Transport(_)), "{error:?}");
        assert_eq!(connections.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn ask_keeps_the_status_of_a_response_that_fails_in_its_body() {
    let cases: [(&[u8], u16); 2] = [
        (
            b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 100\r\n\r\n{",
            401,
        ),
        (
            b"HTTP/1.1 429 Too Many Requests\r\nretry-after: 31\r\ncontent-length: 100\r\n\r\n{",
            429,
        ),
    ];
    for (partial_response, status) in cases {
        let (client, connections) = partial_api(partial_response).await;

        let error = client.ask(noul(INSTRUCTIONS), STATE).await.unwrap_err();

        assert!(
            matches!(error, AskError::Http { status: s, .. } if s == status),
            "{status}: {error:?}"
        );
        assert_eq!(connections.load(Ordering::SeqCst), 1, "{status}");
    }
}

#[tokio::test]
async fn ask_sends_no_more_attempts_when_dropped_in_a_wait() {
    let first = ResponseTemplate::new(503).insert_header("retry-after-ms", "1000");
    let server = mock_api_then(first, noul_response(0.92)).await;

    // wide margins, so a busy machine cannot run the retry before the drop
    let asking = client_for(&server).ask(noul(INSTRUCTIONS), STATE);
    timeout(Duration::from_millis(100), asking)
        .await
        .expect_err("the request must still wait for its retry");
    sleep(Duration::from_millis(1200)).await;

    assert_eq!(request_count(&server).await, 1);
}
