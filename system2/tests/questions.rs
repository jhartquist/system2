//! Tests of `questions!`, `Questions`, and the derive of `Answers`.

mod support;

use serde_json::{Value, json};
use support::{
    Counted, STATE, answer_set_response, api_requests, client_for, counted_drops, instructions,
    mock_api, no_request_sent, noul, noul_answer, only_request, request_count, request_text,
};
use system2::prelude::*;
use system2::{Choice, ChoiceAnswer, Client, Noul, NoulAnswer, QuestionError, Score, ScoreAnswer};

/// The answers of a review of a function.
#[derive(Debug, Answers)]
pub struct Review {
    /// Is there a bug in this code?
    #[system2(question = BUGGY)]
    pub buggy: NoulAnswer,
    /// Is the function tested?
    #[system2(question = noul!("Is the function tested?"))]
    pub tested: NoulAnswer,
}

const REVIEW: Questions<Review> = Questions::new();

impl Review {
    fn is_risky(&self) -> bool {
        self.buggy.is_likely() && !self.tested.is_likely()
    }
}

const REQUEST_TEXT: &str = concat!(
    r#"{"model":"jev-latest","state":"Buy now!","questions":{"#,
    r#""q0":{"type":"noul","instructions":"Is there a bug in this code?"},"#,
    r#""q1":{"type":"noul","instructions":"Is the function tested?"}}}"#,
);

fn review_response() -> wiremock::ResponseTemplate {
    answer_set_response(&[&noul_answer(0.92), &noul_answer(0.1)])
}

#[tokio::test]
async fn ask_sends_a_named_set_in_one_request() {
    let server = mock_api(review_response()).await;

    let review = client_for(&server).ask(REVIEW, STATE).await.unwrap();

    assert_eq!(review.buggy.probability(), 0.92);
    assert_eq!(review.tested.probability(), 0.1);
    assert!(review.is_risky());
    assert_eq!(request_text(&only_request(&server).await), REQUEST_TEXT);
}

#[tokio::test]
async fn ask_finds_the_named_set_from_the_answer_type() {
    let server = mock_api(review_response()).await;

    let review: Review = client_for(&server)
        .ask(Questions::new(), STATE)
        .await
        .unwrap();

    assert_eq!(review.buggy.probability(), 0.92);
    assert_eq!(request_text(&only_request(&server).await), REQUEST_TEXT);
}

#[tokio::test]
async fn ask_makes_the_questions_again_for_each_request() {
    let server = mock_api(review_response()).await;
    let client = client_for(&server);
    let question_set: Questions<Review> = Questions::new();

    let first = client.ask(question_set.clone(), STATE).await.unwrap();
    let second = client.ask(question_set, STATE).await.unwrap();

    assert_eq!(first.buggy.probability(), second.buggy.probability());
    let requests = api_requests(&server).await;
    let texts: Vec<String> = requests.iter().map(request_text).collect();
    assert_eq!(texts, [REQUEST_TEXT, REQUEST_TEXT]);
}

#[derive(Answers)]
struct Triage {
    #[system2(question = Questions::new())]
    review: Review,
    #[system2(question = PROBLEM)]
    problem: ChoiceAnswer<Problem>,
    #[system2(question = score!("How severe is it?", [("Cosmetic.", 1), ("Data is lost.", 9)]))]
    severity: ScoreAnswer<u32>,
    #[system2(question = [BUGGY, TESTED])]
    checks: [NoulAnswer; 2],
    #[system2(question = None::<Noul>)]
    skipped: Option<NoulAnswer>,
    #[system2(question = TESTED.tag("tested"))]
    tagged: (NoulAnswer, &'static str),
}

#[tokio::test]
async fn ask_gives_each_field_the_answers_of_its_question_set() {
    let server = mock_api(answer_set_response(&[
        &noul_answer(0.1),
        &noul_answer(0.2),
        &problem_answer(),
        &severity_answer(),
        &noul_answer(0.3),
        &noul_answer(0.4),
        &noul_answer(0.5),
    ]))
    .await;

    let triage: Triage = client_for(&server)
        .ask(Questions::new(), STATE)
        .await
        .unwrap();

    assert_eq!(triage.review.buggy.probability(), 0.1);
    assert_eq!(triage.review.tested.probability(), 0.2);
    assert_eq!(*triage.problem.most_likely(), Problem::Style);
    assert_eq!(*triage.severity.most_likely(), 9);
    assert_eq!(triage.checks.map(|c| c.probability()), [0.3, 0.4]);
    assert!(triage.skipped.is_none());
    assert_eq!(triage.tagged.0.probability(), 0.5);
    assert_eq!(triage.tagged.1, "tested");
    assert_eq!(instructions(&only_request(&server).await).len(), 7);
}

#[derive(Answers)]
struct Wide {
    #[system2(question = noul!("Is it 0?"))]
    q0: NoulAnswer,
    #[system2(question = noul!("Is it 1?"))]
    q1: NoulAnswer,
    #[system2(question = noul!("Is it 2?"))]
    q2: NoulAnswer,
    #[system2(question = noul!("Is it 3?"))]
    q3: NoulAnswer,
    #[system2(question = noul!("Is it 4?"))]
    q4: NoulAnswer,
    #[system2(question = noul!("Is it 5?"))]
    q5: NoulAnswer,
    #[system2(question = noul!("Is it 6?"))]
    q6: NoulAnswer,
    #[system2(question = noul!("Is it 7?"))]
    q7: NoulAnswer,
    #[system2(question = noul!("Is it 8?"))]
    q8: NoulAnswer,
    #[system2(question = noul!("Is it 9?"))]
    q9: NoulAnswer,
    #[system2(question = noul!("Is it 10?"))]
    q10: NoulAnswer,
    #[system2(question = noul!("Is it 11?"))]
    q11: NoulAnswer,
    #[system2(question = noul!("Is it 12?"))]
    q12: NoulAnswer,
}

#[tokio::test]
async fn ask_takes_a_named_set_of_more_than_12_fields_in_order() {
    let probabilities: Vec<f64> = (0..13).map(|index| f64::from(index) / 100.0).collect();
    let answers: Vec<Value> = probabilities.iter().map(|&p| noul_answer(p)).collect();
    let answers: Vec<&Value> = answers.iter().collect();
    let server = mock_api(answer_set_response(&answers)).await;

    let wide: Wide = client_for(&server)
        .ask(Questions::new(), STATE)
        .await
        .unwrap();

    let read = [
        wide.q0, wide.q1, wide.q2, wide.q3, wide.q4, wide.q5, wide.q6, wide.q7, wide.q8, wide.q9,
        wide.q10, wide.q11, wide.q12,
    ]
    .map(|answer| answer.probability());
    assert_eq!(read.to_vec(), probabilities);
    let instructions = instructions(&only_request(&server).await);
    let sent: Vec<String> = (0..13).map(|index| format!("Is it {index}?")).collect();
    assert_eq!(instructions, sent);
}

#[derive(Answers)]
struct Picks {
    #[system2(question = choice!("Which one?", [("first", Counted), ("second", Counted)]))]
    choice: ChoiceAnswer<Counted>,
    #[system2(question = score!("How bad?", [("Nothing breaks.", Counted), ("Data is lost.", Counted)]))]
    score: ScoreAnswer<Counted>,
}

#[tokio::test]
async fn ask_makes_the_values_of_a_named_set_again_for_its_answers() {
    let server = mock_api(answer_set_response(&[
        &json!({
            "type": "choice",
            "choice": "second",
            "confidence": 1.0,
            "probabilities": { "first": 0.0, "second": 1.0 },
        }),
        &json!({
            "type": "score",
            "score": 0.5,
            "confidence": 0.5,
            "probabilities": { "0": 0.5, "1": 0.5 },
        }),
    ]))
    .await;
    let client = client_for(&server);

    // the request drops the values that it sends, so a dropped request holds none
    let cancelled = client.ask(Questions::<Picks>::new(), STATE);
    assert_eq!(counted_drops(), 4);
    drop(cancelled);
    assert_eq!(counted_drops(), 4);

    let Picks { choice, score } = client.ask(Questions::new(), STATE).await.unwrap();
    assert_eq!(counted_drops(), 8);
    drop(choice);
    assert_eq!(counted_drops(), 10);
    drop(score);
    assert_eq!(counted_drops(), 12);
}

#[derive(Debug, PartialEq, Options)]
enum Problem {
    Bug,
    Style,
}

const BUGGY: Noul = noul!("Is there a bug in this code?");
const TESTED: Noul = noul!("Is the function tested?");
const PROBLEM: Choice<Problem> = choice!("What is the worst problem in this code?", Problem);
const SEVERITY: Score<&str> = score!("How severe is it?", ["Cosmetic.", "Data is lost."]);

fn problem_answer() -> Value {
    json!({
        "type": "choice",
        "choice": "style",
        "confidence": 0.6,
        "probabilities": { "bug": 0.4, "style": 0.6 },
    })
}

// a public set can hold the answer of a private type, as each struct can
/// The verdict of a review: the kind of its problem.
#[derive(Answers)]
pub struct Verdict {
    #[system2(question = PROBLEM)]
    problem: ChoiceAnswer<Problem>,
}

#[tokio::test]
async fn ask_takes_a_public_named_set_with_a_private_answer_type() {
    let server = mock_api(answer_set_response(&[&problem_answer()])).await;

    let verdict: Verdict = client_for(&server)
        .ask(Questions::new(), STATE)
        .await
        .unwrap();

    assert_eq!(verdict.problem.most_likely(), &Problem::Style);
}

fn severity_answer() -> Value {
    json!({
        "type": "score",
        "score": 0.9,
        "confidence": 0.9,
        "probabilities": { "0": 0.1, "1": 0.9 },
    })
}

struct Change {
    id: u32,
}

#[tokio::test]
async fn questions_asks_its_question_sets_in_one_request() {
    let server = mock_api(answer_set_response(&[
        &noul_answer(0.92),
        &problem_answer(),
        &severity_answer(),
        &noul_answer(0.3),
    ]))
    .await;

    let review = client_for(&server)
        .ask(
            questions! {
                buggy: BUGGY,
                problem: PROBLEM,
                severity: score!("How severe is it?", ["Cosmetic.", "Data is lost."]),
                style: questions! { idiomatic: noul!("Is the code idiomatic?") },
            },
            STATE,
        )
        .await
        .unwrap();

    assert_eq!(review.buggy.probability(), 0.92);
    assert_eq!(*review.problem.most_likely(), Problem::Style);
    assert_eq!(*review.severity.most_likely(), "Data is lost.");
    assert_eq!(review.style.idiomatic.probability(), 0.3);
    assert_eq!(
        instructions(&only_request(&server).await),
        [
            "Is there a bug in this code?",
            "What is the worst problem in this code?",
            "How severe is it?",
            "Is the code idiomatic?",
        ]
    );
}

#[tokio::test]
async fn questions_takes_data_along_with_a_tag() {
    let server = mock_api(answer_set_response(&[
        &noul_answer(0.92),
        &noul_answer(0.1),
    ]))
    .await;
    let client = client_for(&server);
    let change = Change { id: 7 };

    let (review, id) = client
        .ask(
            questions! { buggy: BUGGY, tested: TESTED }.tag(change.id),
            STATE,
        )
        .await
        .unwrap();

    assert_eq!(review.buggy.probability(), 0.92);
    assert_eq!(review.tested.probability(), 0.1);
    assert_eq!(id, 7);
}

struct Rule {
    text: String,
}

#[tokio::test]
async fn questions_asks_fields_from_data() -> system2::Result<()> {
    let server = mock_api(answer_set_response(&[
        &noul_answer(0.1),
        &noul_answer(0.8),
        &json!({
            "type": "choice",
            "choice": "infra",
            "confidence": 0.9,
            "probabilities": { "web": 0.1, "infra": 0.9 },
        }),
        &severity_answer(),
    ]))
    .await;
    let rules = [
        Rule {
            text: "No unwrap.".to_owned(),
        },
        Rule {
            text: "No panics.".to_owned(),
        },
    ];
    let teams = ["web", "infra"];

    let checks = rules
        .iter()
        .map(|rule| Ok(Noul::new(&rule.text)?.tag(rule)))
        .collect::<Result<Vec<_>, QuestionError>>()?;
    let review = client_for(&server)
        .ask(
            questions! {
                checks,
                team: Choice::new("Which team owns this code?", &teams)?,
                severity: SEVERITY,
            },
            STATE,
        )
        .await?;

    let checks: Vec<(f64, &str)> = review
        .checks
        .iter()
        .map(|(broken, rule)| (broken.probability(), rule.text.as_str()))
        .collect();
    assert_eq!(checks, [(0.1, "No unwrap."), (0.8, "No panics.")]);
    assert_eq!(**review.team.most_likely(), "infra");
    assert_eq!(*review.severity.most_likely(), "Data is lost.");
    assert_eq!(
        instructions(&only_request(&server).await),
        [
            "No unwrap.",
            "No panics.",
            "Which team owns this code?",
            "How severe is it?"
        ]
    );
    Ok(())
}

#[tokio::test]
async fn questions_returns_the_error_of_a_field_before_a_request() {
    let server = mock_api(answer_set_response(&[])).await;
    let client = client_for(&server);

    let result = async {
        let question_set = questions! { buggy: BUGGY, rule: Noul::new(" ")? };
        client.ask(question_set, STATE).await?;
        Ok::<_, system2::Error>(())
    }
    .await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "the instructions must not be empty"
    );
    assert!(no_request_sent(&server).await);
}

#[tokio::test]
async fn questions_evaluates_each_field_one_time_in_order() {
    let server = mock_api(answer_set_response(&[
        &noul_answer(0.92),
        &noul_answer(0.1),
    ]))
    .await;
    let mut order = Vec::new();

    let answers = client_for(&server)
        .ask(
            questions! {
                buggy: { order.push("buggy"); BUGGY },
                skipped: { order.push("skipped"); None::<Noul> },
                tested: { order.push("tested"); Some(BUGGY) },
            },
            STATE,
        )
        .await
        .unwrap();

    assert_eq!(order, ["buggy", "skipped", "tested"]);
    assert!(answers.skipped.is_none());
    assert_eq!(instructions(&only_request(&server).await).len(), 2);
}

async fn ask_generic<Q: QuestionSet>(
    client: &Client,
    question_set: Q,
) -> system2::Result<Q::AnswerSet> {
    let answers = client.ask(questions! { question_set }, STATE).await?;
    Ok(answers.question_set)
}

#[tokio::test]
async fn questions_keeps_a_borrowed_temporary_to_the_end_of_the_statement() {
    let server = mock_api(answer_set_response(&[
        &json!({
            "type": "choice",
            "choice": "web",
            "confidence": 1.0,
            "probabilities": { "web": 1.0 },
        }),
        &json!({
            "type": "choice",
            "choice": "infra",
            "confidence": 1.0,
            "probabilities": { "infra": 1.0 },
        }),
    ]))
    .await;

    let teams = client_for(&server)
        .ask(
            questions! {
                team: Choice::new("Which team owns this code?", &[String::from("web")]).unwrap(),
                style: questions! {
                    team: Choice::new("Which team owns the style?", &[String::from("infra")])
                        .unwrap(),
                },
            },
            STATE,
        )
        .await
        .map(|answers| {
            let team = answers.team.most_likely().as_str();
            let style_team = answers.style.team.most_likely().as_str();
            (team.to_owned(), style_team.to_owned())
        })
        .unwrap();

    assert_eq!(teams, ("web".to_owned(), "infra".to_owned()));
    assert_eq!(instructions(&only_request(&server).await).len(), 2);
}

#[tokio::test]
async fn questions_asks_a_field_of_a_generic_question_set() {
    let server = mock_api(answer_set_response(&[&noul_answer(0.92)])).await;

    let buggy = ask_generic(&client_for(&server), BUGGY).await.unwrap();

    assert_eq!(buggy.probability(), 0.92);
}

#[tokio::test]
async fn questions_keeps_the_names_of_the_caller() {
    // a binding of the expansion with one of these names would match the constant
    #[allow(non_upper_case_globals, reason = "the names of the bindings")]
    mod caller {
        use system2::Noul;

        pub(super) const field: Option<Noul> = None;
        pub(super) const fields: Option<Noul> = None;
        pub(super) const raw_question_set: Option<Noul> = None;
        pub(super) const raw_answer_set: Option<Noul> = None;
    }
    use caller::{field, fields, raw_answer_set, raw_question_set};

    let server = mock_api(answer_set_response(&[
        &noul_answer(0.92),
        &noul_answer(0.5),
        &noul_answer(0.1),
        &noul_answer(0.2),
    ]))
    .await;
    let tested = noul("Is the code tested?");

    let answers = client_for(&server)
        .ask(
            questions! {
                tested: BUGGY,
                checked: tested,
                review: Questions::<Review>::new(),
                constants: [field, fields, raw_question_set, raw_answer_set],
            },
            STATE,
        )
        .await
        .unwrap();

    assert_eq!(answers.tested.probability(), 0.92);
    assert_eq!(answers.checked.probability(), 0.5);
    assert_eq!(answers.review.buggy.probability(), 0.1);
    assert!(answers.constants.iter().all(Option::is_none));
}

#[tokio::test]
async fn questions_clones_a_question_set() {
    let server = mock_api(answer_set_response(&[&noul_answer(0.92)])).await;
    let client = client_for(&server);
    let question_set = questions! { buggy: BUGGY };

    let first = client.ask(question_set.clone(), STATE).await.unwrap();
    let second = client.ask(question_set, STATE).await.unwrap();

    assert_eq!(first.buggy.probability(), second.buggy.probability());
    assert_eq!(request_count(&server).await, 2);
}

#[tokio::test]
async fn questions_answers_serialize_clone_and_debug() {
    let server = mock_api(answer_set_response(&[&noul_answer(0.92)])).await;

    let answers = client_for(&server)
        .ask(questions! { buggy: BUGGY, skipped: None::<Noul> }, STATE)
        .await
        .unwrap();

    assert_eq!(
        serde_json::to_value(answers.clone()).unwrap(),
        json!({ "buggy": { "probability": 0.92 }, "skipped": null })
    );
    assert_eq!(
        format!("{answers:?}"),
        format!("Answers {{ buggy: {:?}, skipped: None }}", answers.buggy)
    );
}

#[tokio::test]
async fn questions_sends_no_request_for_no_question() {
    let server = mock_api(answer_set_response(&[])).await;
    let client = client_for(&server);

    let empty = client
        .ask(
            questions! { checks: Vec::<Noul>::new(), skipped: None::<Noul> },
            STATE,
        )
        .await
        .unwrap();
    let nothing = client.ask(questions! {}, STATE).await.unwrap();

    assert!(empty.checks.is_empty());
    assert!(empty.skipped.is_none());
    assert_eq!(serde_json::to_value(nothing).unwrap(), json!({}));
    assert!(no_request_sent(&server).await);
}
