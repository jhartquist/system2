mod support;

use serde::Serialize;
use serde_json::{Value, json};
use std::error::Error;
use std::fs;
use std::io::ErrorKind;
use std::time::Duration;
use support::{
    INSTRUCTIONS, STATE, answer_set_body, api_requests, cache_lines, cache_path, client_for,
    mock_api, mock_api_then, no_request_sent, noul, noul_answer, noul_cache_line, noul_response,
    only_request, request_count, request_text, response, sha256_hex,
};
use system2::{AskError, CacheError, Client};
use tokio::time::{sleep, timeout};
use wiremock::ResponseTemplate;

#[tokio::test]
async fn ask_answers_an_equal_request_from_the_cache() {
    let path = cache_path("ask_answers_an_equal_request_from_the_cache");
    let server = mock_api(noul_response(0.92)).await;
    let client = client_for(&server).cache(&path).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "");

    for state in [STATE, STATE, "Hello"] {
        let answer = client.clone().ask(noul(INSTRUCTIONS), state).await.unwrap();
        assert_eq!(answer.probability(), 0.92, "{state}");
    }

    let requests = api_requests(&server).await;
    assert_eq!(requests.len(), 2);
    let response = answer_set_body(&[&noul_answer(0.92)]);
    let lines: Vec<Value> = requests
        .iter()
        .map(|request| {
            let request_sha256 = sha256_hex(&request_text(request));
            json!({ "request_sha256": request_sha256, "response": response })
        })
        .collect();
    assert_eq!(cache_lines(&path), lines);

    // a second run
    server.reset().await;
    let client = client_for(&server).cache(&path).unwrap();
    for state in [STATE, "Hello"] {
        let answer = client.ask(noul(INSTRUCTIONS), state).await.unwrap();
        assert_eq!(answer.probability(), 0.92, "{state}");
    }
    assert!(no_request_sent(&server).await);
    assert_eq!(cache_lines(&path), lines);
}

#[tokio::test]
async fn ask_needs_no_api_key_for_an_answer_from_the_cache() {
    let path = cache_path("ask_needs_no_api_key_for_an_answer_from_the_cache");
    let server = mock_api(noul_response(0.92)).await;
    let client = client_for(&server).cache(&path).unwrap();
    client.ask(noul(INSTRUCTIONS), STATE).await.unwrap();
    let client = Client::new().base_url(server.uri()).cache(&path).unwrap();

    let answer = client.ask(noul(INSTRUCTIONS), STATE).await.unwrap();
    let error = client.ask(noul(INSTRUCTIONS), "Hello").await.unwrap_err();

    assert_eq!(answer.probability(), 0.92);
    assert!(matches!(error, AskError::MissingApiKey), "{error:?}");
    assert_eq!(request_count(&server).await, 1);
}

#[tokio::test]
async fn ask_checks_an_answer_from_the_cache() {
    let path = cache_path("ask_checks_an_answer_from_the_cache");
    let cases = [
        (
            answer_set_body(&[&noul_answer(1.5)]),
            "the response must give a probability from 0 to 1, not 1.5",
        ),
        (
            json!({ "model": "jev-1.13.0" }),
            "the response must have the shape of the API: missing field `answers`",
        ),
    ];
    for (response, message) in cases {
        fs::write(&path, noul_cache_line(STATE, &response)).unwrap();

        let error = Client::new()
            .cache(&path)
            .unwrap()
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
async fn ask_adds_only_a_response_whose_answers_pass() {
    let cases = [
        ResponseTemplate::new(400).set_body_string("Bad request"),
        ResponseTemplate::new(200).set_body_string("<html>"),
        response(&noul_answer(1.5)),
    ];
    for (index, response) in cases.into_iter().enumerate() {
        let path = cache_path(&format!(
            "ask_adds_only_a_response_whose_answers_pass_{index}"
        ));
        let server = mock_api(response).await;
        let client = client_for(&server).cache(&path).unwrap();

        for _ in 0..2 {
            client.ask(noul(INSTRUCTIONS), STATE).await.unwrap_err();
        }

        assert_eq!(request_count(&server).await, 2, "{index}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "", "{index}");
    }
}

#[tokio::test]
async fn ask_answers_a_deep_state_from_the_cache() {
    let path = cache_path("ask_answers_a_deep_state_from_the_cache");
    let server = mock_api(noul_response(0.92)).await;

    // deeper than the 128 levels that serde_json reads into a `Value`
    let mut state = json!(STATE);
    for _ in 0..200 {
        state = json!([state]);
    }
    let online = client_for(&server).cache(&path).unwrap();
    online.ask(noul(INSTRUCTIONS), &state).await.unwrap();
    let offline = Client::new().cache(&path).unwrap().offline(true);

    let answer = offline.ask(noul(INSTRUCTIONS), &state).await.unwrap();

    assert_eq!(answer.probability(), 0.92);
    assert_eq!(request_count(&server).await, 1);
}

#[tokio::test]
async fn ask_answers_a_deep_response_from_the_cache() {
    let path = cache_path("ask_answers_a_deep_response_from_the_cache");

    // as deep as serde_json reads, so the line around it is one level too deep to read as one
    let depth = 126;
    let extra = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
    let text = format!(r#"{{"answers":{{"q0":{{"type":"noul","noul":0.92}}}},"extra":{extra}}}"#);
    let server = mock_api(ResponseTemplate::new(200).set_body_string(text)).await;
    let online = client_for(&server).cache(&path).unwrap();
    online.ask(noul(INSTRUCTIONS), STATE).await.unwrap();
    let offline = Client::new().cache(&path).unwrap().offline(true);

    let answer = offline.ask(noul(INSTRUCTIONS), STATE).await.unwrap();

    assert_eq!(answer.probability(), 0.92);
}

#[tokio::test]
async fn ask_keeps_each_digit_of_a_number_in_the_cache() {
    #[derive(Serialize)]
    struct Ticket {
        id: u128,
    }

    let path = cache_path("ask_keeps_each_digit_of_a_number_in_the_cache");
    let server = mock_api_then(noul_response(0.1), noul_response(0.9)).await;
    let online = client_for(&server).cache(&path).unwrap();

    // a `Value` rounds each of these ids to the same `f64`
    let tickets = [18_446_744_073_709_551_616, 18_446_744_073_709_551_617].map(|id| Ticket { id });
    for ticket in &tickets {
        online.ask(noul(INSTRUCTIONS), ticket).await.unwrap();
    }
    let offline = Client::new().cache(&path).unwrap().offline(true);

    let first = offline.ask(noul(INSTRUCTIONS), &tickets[0]).await.unwrap();
    let second = offline.ask(noul(INSTRUCTIONS), &tickets[1]).await.unwrap();

    assert_eq!((first.probability(), second.probability()), (0.1, 0.9));
    assert_eq!(request_count(&server).await, 2);
}

#[tokio::test]
async fn ask_answers_with_the_first_line_of_a_request() {
    let path = cache_path("ask_answers_with_the_first_line_of_a_request");
    let lines = [0.1, 0.9]
        .map(|probability| noul_cache_line(STATE, &answer_set_body(&[&noul_answer(probability)])));
    fs::write(&path, lines.concat()).unwrap();

    let answer = Client::new()
        .cache(&path)
        .unwrap()
        .ask(noul(INSTRUCTIONS), STATE)
        .await
        .unwrap();

    assert_eq!(answer.probability(), 0.1);
}

#[tokio::test]
async fn ask_adds_one_line_for_a_request_asked_twice_at_once() {
    let path = cache_path("ask_adds_one_line_for_a_request_asked_twice_at_once");
    let server = mock_api(noul_response(0.92)).await;
    let client = client_for(&server).cache(&path).unwrap();

    // `join!` polls both before either response, so both miss
    let (first, second) = tokio::join!(
        client.ask(noul(INSTRUCTIONS), STATE),
        client.ask(noul(INSTRUCTIONS), STATE),
    );

    first.unwrap();
    second.unwrap();
    assert_eq!(request_count(&server).await, 2);
    assert_eq!(cache_lines(&path).len(), 1);
}

#[tokio::test]
async fn ask_writes_nothing_to_the_cache_when_dropped() {
    let path = cache_path("ask_writes_nothing_to_the_cache_when_dropped");
    let server = mock_api(noul_response(0.92).set_delay(Duration::from_secs(1))).await;
    let client = client_for(&server).cache(&path).unwrap();

    // wide margins, so a busy machine sends the request before the drop, and the response
    // arrives before the check
    let asking = client.ask(noul(INSTRUCTIONS), STATE);
    timeout(Duration::from_millis(300), asking)
        .await
        .unwrap_err();
    sleep(Duration::from_secs(1)).await;

    assert_eq!(request_count(&server).await, 1);
    assert_eq!(fs::read_to_string(&path).unwrap(), "");
}

#[tokio::test]
async fn ask_answers_from_the_cache_with_no_place_in_the_limit() {
    let path = cache_path("ask_answers_from_the_cache_with_no_place_in_the_limit");
    fs::write(
        &path,
        noul_cache_line(STATE, &answer_set_body(&[&noul_answer(0.92)])),
    )
    .unwrap();
    let server = mock_api(noul_response(0.1).set_delay(Duration::from_secs(2))).await;
    let client = client_for(&server)
        .max_concurrent_requests(1)
        .cache(&path)
        .unwrap();

    let slow = tokio::spawn(client.ask(noul(INSTRUCTIONS), "Hello"));
    sleep(Duration::from_millis(200)).await;
    let cached = timeout(
        Duration::from_secs(1),
        client.ask(noul(INSTRUCTIONS), STATE),
    )
    .await;

    assert_eq!(cached.unwrap().unwrap().probability(), 0.92);
    assert_eq!(slow.await.unwrap().unwrap().probability(), 0.1);
}

#[tokio::test]
async fn cache_gives_a_clone_a_cache_of_its_own() {
    let first_path = cache_path("cache_gives_a_clone_a_cache_of_its_own_first");
    let second_path = cache_path("cache_gives_a_clone_a_cache_of_its_own_second");
    let server = mock_api(noul_response(0.92)).await;
    let first = client_for(&server).cache(&first_path).unwrap();
    let second = first.clone().cache(&second_path).unwrap();

    second.ask(noul(INSTRUCTIONS), STATE).await.unwrap();
    first.ask(noul(INSTRUCTIONS), STATE).await.unwrap();

    assert_eq!(request_count(&server).await, 2);
    assert_eq!(cache_lines(&first_path).len(), 1);
    assert_eq!(cache_lines(&second_path).len(), 1);
}

#[tokio::test]
async fn cache_ends_a_last_line_that_has_no_newline() {
    let path = cache_path("cache_ends_a_last_line_that_has_no_newline");
    let line = noul_cache_line(STATE, &answer_set_body(&[&noul_answer(0.1)]));
    fs::write(&path, line.trim_end()).unwrap();
    let server = mock_api(noul_response(0.92)).await;

    client_for(&server)
        .cache(&path)
        .unwrap()
        .ask(noul(INSTRUCTIONS), "Hello")
        .await
        .unwrap();
    let offline = Client::new().cache(&path).unwrap().offline(true);

    let first = offline.ask(noul(INSTRUCTIONS), STATE).await.unwrap();
    let second = offline.ask(noul(INSTRUCTIONS), "Hello").await.unwrap();

    assert_eq!((first.probability(), second.probability()), (0.1, 0.92));
    assert_eq!(cache_lines(&path).len(), 2);
}

#[tokio::test]
async fn cache_answers_from_a_file_that_it_can_only_read() {
    let path = cache_path("cache_answers_from_a_file_that_it_can_only_read");
    let line = noul_cache_line(STATE, &answer_set_body(&[&noul_answer(0.92)]));
    fs::write(&path, &line).unwrap();
    let writable = fs::metadata(&path).unwrap().permissions();
    let mut read_only = writable.clone();
    read_only.set_readonly(true);
    fs::set_permissions(&path, read_only).unwrap();
    let server = mock_api(noul_response(0.1)).await;
    let online = client_for(&server).cache(&path).unwrap();
    let offline = online.clone().offline(true);

    let answer = offline.ask(noul(INSTRUCTIONS), STATE).await.unwrap();
    let offline_error = offline.ask(noul(INSTRUCTIONS), "Hello").await.unwrap_err();
    let online_error = online.ask(noul(INSTRUCTIONS), "Hello").await.unwrap_err();

    // a later run can remove the file
    fs::set_permissions(&path, writable).unwrap();
    assert_eq!(answer.probability(), 0.92);
    assert!(
        matches!(offline_error, AskError::Offline),
        "{offline_error:?}"
    );
    assert!(
        matches!(online_error, AskError::Cache(CacheError::Write { .. })),
        "{online_error:?}"
    );
    assert_eq!(
        online_error.to_string(),
        format!(
            "the client cannot add to the cache file `{}`",
            path.display()
        )
    );
    assert_eq!(
        online_error.source().unwrap().to_string(),
        "permission denied"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), line);
    assert_eq!(request_count(&server).await, 0);
}

#[test]
fn cache_refuses_a_line_that_is_not_a_hash_and_its_response() {
    let path = cache_path("cache_refuses_a_line_that_is_not_a_hash_and_its_response");
    let valid = format!(
        r#"{{"request_sha256":"{}","response":{{}}}}"#,
        sha256_hex("{}")
    );
    let hash = sha256_hex("{}");
    let bad_hash = "z".repeat(64);
    let cases: [(Vec<u8>, _, _); 5] = [
        (b"not json\n".to_vec(), 1, "expected ident"),
        (
            format!("{valid}\n{{\"request_sha256\":\"{hash}\"}}\n").into(),
            2,
            "missing field `response`",
        ),
        (
            format!("{valid}\n{{\"request_sha256\":\"{bad_hash}\",\"response\":{{}}}}\n").into(),
            2,
            "Invalid character 'z' at position 0",
        ),
        (
            format!("{valid}\n\n{valid}\n").into(),
            2,
            "EOF while parsing a value",
        ),
        (
            [valid.as_bytes(), b"\n\xff\n"].concat(),
            2,
            "expected value",
        ),
    ];
    for (text, line, message) in cases {
        fs::write(&path, &text).unwrap();

        let error = Client::new().cache(&path).unwrap_err();

        assert!(
            matches!(&error, CacheError::InvalidLine { line: number, .. } if *number == line),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            format!(
                "line {line} of the cache file `{}` must be the hash of a request and its \
                 response",
                path.display()
            )
        );
        let source = error.source().unwrap().to_string();
        assert!(source.starts_with(message), "{source}");
        assert_eq!(fs::read(&path).unwrap(), text);
    }
}

#[test]
fn cache_refuses_a_file_that_it_cannot_read() {
    let folder = cache_path("cache_refuses_a_file_that_it_cannot_read").with_extension("");
    let _ = fs::remove_dir(&folder);
    fs::create_dir(&folder).unwrap();
    let cases = [
        (
            folder.join("missing").join("cache.jsonl"),
            ": make its folder first",
        ),
        (folder.clone(), ""),
    ];
    for (path, fix) in cases {
        let error = Client::new().cache(&path).unwrap_err();

        assert!(matches!(error, CacheError::Read { .. }), "{error:?}");
        assert_eq!(
            error.to_string(),
            format!(
                "the client cannot read the cache file `{}`{fix}",
                path.display()
            )
        );
        assert!(error.source().is_some());
    }
}

// a read-only folder on windows still lets the client make a file
#[cfg(unix)]
#[test]
fn cache_refuses_a_new_file_in_a_folder_that_it_can_only_read() {
    let folder =
        cache_path("cache_refuses_a_new_file_in_a_folder_that_it_can_only_read").with_extension("");
    let _ = fs::remove_dir(&folder);
    fs::create_dir(&folder).unwrap();
    let writable = fs::metadata(&folder).unwrap().permissions();
    let mut read_only = writable.clone();
    read_only.set_readonly(true);
    fs::set_permissions(&folder, read_only).unwrap();
    let path = folder.join("cache.jsonl");

    let error = Client::new().cache(&path).unwrap_err();

    fs::set_permissions(&folder, writable).unwrap();
    let CacheError::Read { source, .. } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(source.kind(), ErrorKind::PermissionDenied);
    assert!(!path.exists());
}

#[tokio::test]
async fn offline_answers_only_from_the_cache() {
    let path = cache_path("offline_answers_only_from_the_cache");
    let server = mock_api(noul_response(0.92)).await;
    let client = client_for(&server).cache(&path).unwrap();
    client.ask(noul(INSTRUCTIONS), STATE).await.unwrap();
    let offline = client.clone().offline(true);

    let answer = offline.ask(noul(INSTRUCTIONS), STATE).await.unwrap();
    let errors = [
        offline.ask(noul(INSTRUCTIONS), "Hello").await.unwrap_err(),
        client_for(&server)
            .offline(true)
            .ask(noul(INSTRUCTIONS), STATE)
            .await
            .unwrap_err(),
    ];

    assert_eq!(answer.probability(), 0.92);
    for error in errors {
        assert!(matches!(error, AskError::Offline), "{error:?}");
        assert_eq!(
            error.to_string(),
            "the cache has no response for the request, and the client is offline: \
             set `SYSTEM2_CACHE` or call `Client::cache`, and record the response with a run online"
        );
    }
    only_request(&server).await;
}
