#![allow(clippy::float_cmp, reason = "tests check exact API values")]

mod support;

use serde_json::json;
use std::rc::Rc;
use support::{STATE, client_for, mock_api, only_request, request_text, response};
use system2::{AskError, Choice};
use wiremock::ResponseTemplate;

const REQUEST_ID: &str = "req_01a13d1fbca4728f8f7c1cdd312bd3a5";

#[derive(Debug)]
struct File {
    path: String,
}

fn files() -> Vec<File> {
    ["src/lib.rs", "src/client.rs"]
        .map(|path| File {
            path: path.to_owned(),
        })
        .into()
}

fn file_response() -> ResponseTemplate {
    response(&json!({
        "type": "choice",
        "choice": "src/client.rs",
        "confidence": 0.71,
        "probabilities": { "src/lib.rs": 0.15, "src/client.rs": 0.85 },
    }))
}

#[tokio::test]
async fn ask_sends_the_options_in_order_and_reads_the_answer() {
    let server = mock_api(response(&json!({
        "type": "choice",
        "choice": "block",
        "confidence": 0.64,
        "probabilities": { "approve": 0.18, "block": 0.82 },
    })))
    .await;
    let options = [("block", Some("Ask for a fix.")), ("approve", None)];
    let question = Choice::with_descriptions("What must the reviewer do?", options).unwrap();

    let answer = client_for(&server).ask(question, STATE).await.unwrap();

    assert_eq!(*answer.most_likely(), "block");
    assert_eq!(answer.confidence(), 0.64);
    let request = only_request(&server).await;
    assert_eq!(
        request_text(&request),
        concat!(
            r#"{"model":"jev-latest","state":"Buy now!","questions":{"q0":{"type":"choice","#,
            r#""instructions":"What must the reviewer do?","#,
            r#""criteria":{"block":"Ask for a fix.","approve":null}}}}"#,
        )
    );
}

#[tokio::test]
async fn ask_gives_back_values_that_are_not_clone() {
    let server = mock_api(file_response()).await;
    let client = client_for(&server);
    let files = files();

    let borrowed = Choice::with_values("Which file?", &files, |f| &f.path).unwrap();
    let file: &File = client
        .ask(borrowed, STATE)
        .await
        .unwrap()
        .into_most_likely();
    assert_eq!(file.path, "src/client.rs");

    let owned = Choice::with_values("Which file?", files, |f| f.path.clone()).unwrap();
    let file: File = client.ask(owned, STATE).await.unwrap().into_most_likely();
    assert_eq!(file.path, "src/client.rs");
}

#[tokio::test]
async fn ask_takes_values_that_are_not_send() {
    let server = mock_api(file_response()).await;
    let files: Vec<Rc<File>> = files().into_iter().map(Rc::new).collect();
    let question = Choice::with_values("Which file?", files, |f| f.path.clone()).unwrap();

    let answer = client_for(&server).ask(question, STATE).await.unwrap();

    assert_eq!(answer.most_likely().path, "src/client.rs");
}

#[tokio::test]
async fn ask_reports_an_answer_with_an_unknown_option() {
    let answer = json!({
        "type": "choice",
        "choice": "src/main.rs",
        "confidence": 0.9,
        "probabilities": { "src/lib.rs": 0.05, "src/client.rs": 0.95 },
    });
    let server =
        mock_api(response(&answer).insert_header("x-typesafe-request-id", REQUEST_ID)).await;
    let question = Choice::with_values("Which file?", files(), |f| f.path.clone()).unwrap();

    let error = client_for(&server).ask(question, STATE).await.unwrap_err();

    assert_eq!(
        error.to_string(),
        format!(
            "the response must choose an option of the Choice, not `src/main.rs` (request ID `{REQUEST_ID}`)"
        )
    );
    let AskError::InvalidResponse { request_id, .. } = error else {
        panic!("expected an invalid response, got {error:?}");
    };
    assert_eq!(request_id.as_deref(), Some(REQUEST_ID));
}
