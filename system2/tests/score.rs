mod support;

use serde_json::{Value, json};
use support::{
    Counted, STATE, answer_set_response, client_for, counted_drops, mock_api, only_request,
    question_texts, request_body, request_text, response,
};
use system2::Score;
use system2::prelude::*;

#[derive(Debug)]
struct Risk {
    text: String,
}

#[derive(Debug, PartialEq, Levels)]
enum Severity {
    /// Nothing breaks.
    Minor,
    /// Data is lost
    /// or corrupted.
    Critical,
}

#[derive(Debug, PartialEq, Levels)]
enum Effort {
    Small,
    LargeChange,
}

#[derive(Debug, PartialEq, Levels)]
enum Exposure {
    /// The default for a new change.
    #[system2(description = "Low: the change touches no data.")]
    Low,
    /// The change needs a second review.
    #[system2(description = "High: the change can lose data.")]
    High,
}

const SEVERITY: Score<Severity> = score!("How severe is the problem?", Severity);

const SIZE: Score<&str> = score!(
    "How big is the change?",
    ["A line.", "A function.", "A module."]
);

const DAYS: Score<u32> = score!(
    "How long does the change take?",
    [("A day.", 1), ("A week.", 5)]
);

fn score_answer(probabilities: &[f64]) -> Value {
    let probabilities: serde_json::Map<String, Value> = probabilities
        .iter()
        .enumerate()
        .map(|(index, &probability)| (index.to_string(), json!(probability)))
        .collect();
    json!({
        "type": "score",
        "score": 0.5,
        "confidence": 0.5,
        "probabilities": probabilities,
    })
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

#[tokio::test]
async fn ask_gives_back_a_value_of_the_type() {
    let server = mock_api(response(&json!({
        "type": "score",
        "score": 0.7,
        "confidence": 0.4,
        "probabilities": { "0": 0.3, "1": 0.7 },
    })))
    .await;
    let question = Score::<Severity>::of("How severe is the problem?").unwrap();

    let severity = client_for(&server)
        .ask(question, STATE)
        .await
        .unwrap()
        .into_most_likely();

    assert_eq!(severity, Severity::Critical);
}

#[tokio::test]
async fn ask_sends_the_descriptions_or_the_names_of_the_derive() {
    let server = mock_api(response(&json!({
        "type": "score",
        "score": 0.5,
        "confidence": 0.0,
        "probabilities": { "0": 0.5, "1": 0.5 },
    })))
    .await;
    let client = client_for(&server);

    client
        .ask(Score::<Severity>::of("How severe?").unwrap(), STATE)
        .await
        .unwrap();
    client
        .ask(Score::<Effort>::of("How big?").unwrap(), STATE)
        .await
        .unwrap();
    client
        .ask(Score::<Exposure>::of("How risky?").unwrap(), STATE)
        .await
        .unwrap();

    let criteria: Vec<_> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| request_body(request)["questions"]["q0"]["criteria"].clone())
        .collect();
    assert_eq!(
        criteria,
        [
            json!(["Nothing breaks.", "Data is lost or corrupted."]),
            json!(["small", "large_change"]),
            json!([
                "Low: the change touches no data.",
                "High: the change can lose data."
            ]),
        ]
    );
}

#[tokio::test]
async fn score_sends_each_form_as_written() {
    let unit = "line";
    let answers = [
        score_answer(&[0.0, 1.0]),
        score_answer(&[0.0, 1.0, 0.0]),
        score_answer(&[1.0, 0.0]),
        score_answer(&[0.0, 1.0]),
        score_answer(&[1.0, 0.0]),
    ];
    let server = mock_api(answer_set_response(&answers.iter().collect::<Vec<_>>())).await;

    let (severity, size, days, lines, severity_of_unit) = client_for(&server)
        .ask(
            (
                SEVERITY,
                SIZE,
                DAYS,
                score!("How many {unit}s?", ["A {{few}}.", ("Many.", "many")]),
                score!("How severe is the {unit}?", Severity),
            ),
            STATE,
        )
        .await
        .unwrap();

    assert_eq!(*severity.most_likely(), Severity::Critical);
    assert_eq!(*size.most_likely(), "A function.");
    assert_eq!(days.into_most_likely(), 1);
    assert_eq!(severity_of_unit.into_most_likely(), Severity::Minor);
    assert_eq!(
        lines.into_probabilities(),
        [("A {few}.", 0.0), ("many", 1.0)]
    );
    assert_eq!(
        question_texts(&only_request(&server).await),
        [
            r#"{"type":"score","instructions":"How severe is the problem?","criteria":["Nothing breaks.","Data is lost or corrupted."]}"#,
            r#"{"type":"score","instructions":"How big is the change?","criteria":["A line.","A function.","A module."]}"#,
            r#"{"type":"score","instructions":"How long does the change take?","criteria":["A day.","A week."]}"#,
            r#"{"type":"score","instructions":"How many lines?","criteria":["A {few}.","Many."]}"#,
            r#"{"type":"score","instructions":"How severe is the line?","criteria":["Nothing breaks.","Data is lost or corrupted."]}"#,
        ]
    );
}

#[tokio::test]
async fn score_gives_back_the_values_of_a_constant() {
    let server = mock_api(response(&score_answer(&[0.3, 0.7]))).await;

    let severity = client_for(&server).ask(SEVERITY, STATE).await.unwrap();

    let probabilities: Vec<_> = severity.probabilities().collect();
    assert_eq!(
        probabilities,
        [(&Severity::Minor, 0.3), (&Severity::Critical, 0.7)]
    );
    let question = severity.into_question();
    let again = client_for(&server).ask(question, STATE).await.unwrap();
    assert_eq!(again.into_most_likely(), Severity::Critical);
}

#[tokio::test]
async fn score_keeps_each_value_until_the_answer_drops() {
    let server = mock_api(response(&score_answer(&[0.3, 0.7]))).await;
    let question = score!(
        "How severe?",
        [("Nothing breaks.", Counted), ("Data is lost.", Counted)]
    );

    let ask_future = client_for(&server).ask(question, STATE);
    assert_eq!(counted_drops(), 0);
    let answer = ask_future.await.unwrap();
    assert_eq!(counted_drops(), 0);

    drop(answer);
    assert_eq!(counted_drops(), 2);
}

#[derive(Debug, PartialEq, Levels)]
enum V2 {
    Small,
    Large,
}

fn score_of<L: Levels>() -> Score<L> {
    score!("How severe?", L)
}

#[tokio::test]
async fn score_takes_each_path_to_a_type() {
    let answers = [
        score_answer(&[0.0, 1.0]),
        score_answer(&[0.0, 1.0]),
        score_answer(&[0.0, 1.0]),
        score_answer(&[0.0, 1.0]),
    ];
    let server = mock_api(answer_set_response(&answers.iter().collect::<Vec<_>>())).await;

    let (version, raw, qualified, generic) = client_for(&server)
        .ask(
            (
                score!("How big?", V2),
                score!("How severe?", r#Severity),
                score!("How severe?", crate::Severity),
                score_of::<Severity>(),
            ),
            STATE,
        )
        .await
        .unwrap();

    assert_eq!(version.into_most_likely(), V2::Large);
    for severity in [raw, qualified, generic] {
        assert_eq!(severity.into_most_likely(), Severity::Critical);
    }
}

trait Limit {
    const MAX: u64;
}

struct Big;

impl Limit for Big {
    const MAX: u64 = 5_000_000_000;
}

fn score_up_to<T: Limit>() -> Score<u64> {
    score!("How many?", [("Some.", 1), ("Most.", T::MAX)])
}

#[tokio::test]
async fn score_infers_each_value_from_the_question() {
    const MAYBE: Score<Option<u8>> = score!("How many?", [("None.", None), ("One.", Some(1))]);
    const MANY: Score<u64> = score!("How many?", [("Few.", 1), ("Many.", 5_000_000_000)]);
    let answers = [
        score_answer(&[1.0, 0.0]),
        score_answer(&[0.0, 1.0]),
        score_answer(&[0.0, 1.0]),
    ];
    let server = mock_api(answer_set_response(&answers.iter().collect::<Vec<_>>())).await;

    let (maybe, many, generic) = client_for(&server)
        .ask((MAYBE, MANY, score_up_to::<Big>()), STATE)
        .await
        .unwrap();

    assert_eq!(maybe.into_most_likely(), None);
    assert_eq!(many.into_most_likely(), 5_000_000_000);
    assert_eq!(generic.into_most_likely(), 5_000_000_000);
}
