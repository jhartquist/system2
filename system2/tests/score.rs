mod support;

use serde_json::json;
use support::{STATE, client_for, mock_api, only_request, request_text, response};
use system2::Score;

#[derive(Debug)]
struct Risk {
    text: String,
}

#[tokio::test]
async fn ask_sends_the_levels_in_order_and_reads_the_answer() {
    let server = mock_api(response(&json!({
        "type": "score",
        "score": 1.2,
        "confidence": 0.35,
        "probabilities": { "0": 0.1, "1": 0.6, "2": 0.3 },
        "legend": { "0": "A line.", "1": "A function.", "2": "A module." },
    })))
    .await;
    let question = Score::new(
        "How big is the change?",
        ["A line.", "A function.", "A module."],
    )
    .unwrap();

    let answer = client_for(&server).ask(question, STATE).await.unwrap();

    assert_eq!(*answer.most_likely(), "A function.");
    assert_eq!(answer.score(), 1.2);
    assert_eq!(answer.confidence(), 0.35);
    let request = only_request(&server).await;
    assert_eq!(
        request_text(&request),
        concat!(
            r#"{"model":"jev-latest","state":"Buy now!","questions":{"q0":{"type":"score","#,
            r#""instructions":"How big is the change?","#,
            r#""criteria":["A line.","A function.","A module."]}}}"#,
        )
    );
}

#[tokio::test]
async fn ask_gives_back_values_that_are_not_clone() {
    let server = mock_api(response(&json!({
        "type": "score",
        "score": 0.8,
        "confidence": 0.6,
        "probabilities": { "0": 0.2, "1": 0.8 },
    })))
    .await;
    let risks = ["The change touches no data.", "The change can lose data."].map(|text| Risk {
        text: text.to_owned(),
    });
    let question = Score::with_values("How risky is it?", risks, |r| r.text.clone()).unwrap();

    let risk: Risk = client_for(&server)
        .ask(question, STATE)
        .await
        .unwrap()
        .into_most_likely();

    assert_eq!(risk.text, "The change can lose data.");
}
