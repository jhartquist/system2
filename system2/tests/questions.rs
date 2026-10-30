mod support;

use serde_json::{Value, json};
use std::cell::Cell;
use std::ptr;
use support::{
    Counted, STATE, answer_set_response, client_for, counted_drops, instructions, mock_api,
    no_request_sent, noul, noul_answer, only_request, request_text,
};
use system2::prelude::*;
use system2::{
    Choice, ChoiceAnswer, Client, NamedAnswerSet, Noul, NoulAnswer, QuestionError, Questions,
    Score, ScoreAnswer,
};

struct Review {
    buggy: NoulAnswer,
    language: ChoiceAnswer<&'static str>,
}

impl NamedAnswerSet for Review {
    type QuestionSet = (Noul, Choice<&'static str>);
    type AnswerSet = (NoulAnswer, ChoiceAnswer<&'static str>);

    #[expect(
        clippy::unwrap_used,
        reason = "`questions!` writes `choice!` here, which never fails"
    )]
    fn question_set() -> Self::QuestionSet {
        let buggy = noul("Is there a bug in this code?");
        let language = Choice::new("Which language is the code in?", ["rust", "go"]).unwrap();
        (buggy, language)
    }

    fn from_answer_set((buggy, language): Self::AnswerSet) -> Self {
        Review { buggy, language }
    }
}

const REVIEW: Questions<Review> = Questions::new();

const REQUEST_TEXT: &str = concat!(
    r#"{"model":"jev-latest","state":"Buy now!","questions":{"#,
    r#""q0":{"type":"noul","instructions":"Is there a bug in this code?"},"#,
    r#""q1":{"type":"choice","instructions":"Which language is the code in?","criteria":{"rust":null,"go":null}}}}"#,
);

fn review_response() -> wiremock::ResponseTemplate {
    answer_set_response(&[
        &noul_answer(0.92),
        &json!({
            "type": "choice",
            "choice": "go",
            "confidence": 0.7,
            "probabilities": { "rust": 0.3, "go": 0.7 },
        }),
    ])
}

#[tokio::test]
async fn ask_sends_a_named_set_in_one_request() {
    let server = mock_api(review_response()).await;

    let review = client_for(&server).ask(REVIEW, STATE).await.unwrap();

    assert_eq!(review.buggy.probability(), 0.92);
    assert_eq!(*review.language.most_likely(), "go");
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
    let requests = server.received_requests().await.unwrap();
    let texts: Vec<String> = requests.iter().map(request_text).collect();
    assert_eq!(texts, [REQUEST_TEXT, REQUEST_TEXT]);
}

thread_local! {
    static QUESTION_SET_CALLS: Cell<usize> = const { Cell::new(0) };
}

struct Picks {
    choice: ChoiceAnswer<Counted>,
    score: ScoreAnswer<Counted>,
}

impl NamedAnswerSet for Picks {
    type QuestionSet = (Choice<Counted>, Score<Counted>);
    type AnswerSet = (ChoiceAnswer<Counted>, ScoreAnswer<Counted>);

    fn question_set() -> Self::QuestionSet {
        QUESTION_SET_CALLS.set(QUESTION_SET_CALLS.get() + 1);
        let choice = choice!("Which one?", [("first", Counted), ("second", Counted)]);
        let score = score!(
            "How bad?",
            [("Nothing breaks.", Counted), ("Data is lost.", Counted)]
        );
        (choice, score)
    }

    fn from_answer_set((choice, score): Self::AnswerSet) -> Self {
        Picks { choice, score }
    }
}

fn calls_and_drops() -> (usize, usize) {
    let calls = QUESTION_SET_CALLS.get();
    (calls, counted_drops())
}

#[tokio::test]
async fn ask_keeps_the_values_of_a_named_set_until_their_owner_drops() {
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

    let cancelled = client.ask(Questions::<Picks>::new(), STATE);
    assert_eq!(calls_and_drops(), (1, 0));
    drop(cancelled);
    assert_eq!(calls_and_drops(), (1, 4));

    let Picks { choice, score } = client.ask(Questions::new(), STATE).await.unwrap();
    assert_eq!(calls_and_drops(), (2, 4));
    drop(choice);
    assert_eq!(calls_and_drops(), (2, 6));
    drop(score);
    assert_eq!(calls_and_drops(), (2, 8));
}

#[derive(Debug, PartialEq, Options)]
enum Problem {
    Bug,
    Style,
}

const BUGGY: Noul = noul!("Is there a bug in this code?");
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
    let change = Change { id: 7 };

    let review = client_for(&server)
        .ask(
            questions! {
                buggy: BUGGY,
                problem: PROBLEM,
                severity: score!("How severe is it?", ["Cosmetic.", "Data is lost."]),
                style: questions! { idiomatic: noul!("Is the code idiomatic?") },
                id: change.id,
            },
            STATE,
        )
        .await
        .unwrap();

    assert_eq!(review.buggy.probability(), 0.92);
    assert_eq!(*review.problem.most_likely(), Problem::Style);
    assert_eq!(*review.severity.most_likely(), "Data is lost.");
    assert_eq!(review.style.idiomatic.probability(), 0.3);
    assert_eq!(review.id, 7);
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
                id: { order.push("id"); 7 },
                tested: { order.push("tested"); Some(BUGGY) },
            },
            STATE,
        )
        .await
        .unwrap();

    assert_eq!(order, ["buggy", "id", "tested"]);
    assert_eq!(answers.id, 7);
    assert_eq!(instructions(&only_request(&server).await).len(), 2);
}

#[tokio::test]
async fn questions_asks_a_field_by_its_type() {
    let server = mock_api(answer_set_response(&[&noul_answer(0.92)])).await;
    let text = "change";
    let mut count = 3_u32;

    let answers = client_for(&server)
        .ask(
            questions! {
                asked: Some(BUGGY),
                skipped: None::<Noul>,
                count: Some(count),
                text: &text,
                counted: &mut count,
            },
            STATE,
        )
        .await
        .unwrap();
    *answers.counted += 1;

    assert_eq!(answers.asked.unwrap().probability(), 0.92);
    assert!(answers.skipped.is_none());
    assert_eq!(answers.count, Some(3));
    assert_eq!(*answers.text, "change");
    assert_eq!(count, 4);
    assert_eq!(instructions(&only_request(&server).await).len(), 1);
}

#[tokio::test]
async fn questions_moves_a_value_into_the_answers() {
    let server = mock_api(answer_set_response(&[&noul_answer(0.92)])).await;
    let drops = counted_drops();

    let answers = client_for(&server)
        .ask(questions! { buggy: BUGGY, owner: Counted }, STATE)
        .await
        .unwrap();
    assert_eq!(counted_drops(), drops);
    drop(answers.owner);

    assert_eq!(counted_drops(), drops + 1);
}

async fn ask_with_id<Q: QuestionSet>(
    client: &Client,
    question_set: Q,
) -> system2::Result<(Q::AnswerSet, u32)> {
    let answers = client
        .ask(questions! { question_set, id: 7 }, STATE)
        .await?;
    Ok((answers.question_set, answers.id))
}

#[tokio::test]
async fn questions_moves_a_reference_it_does_not_check_into_the_answers() {
    let server = mock_api(answer_set_response(&[])).await;
    let nouls = [BUGGY];
    let borrowed = &nouls[0];
    let borrowed_twice = &borrowed;

    let answers = client_for(&server)
        .ask(
            questions! {
                deep: &borrowed_twice,
                list: vec![&nouls[0]],
                option: Some(&nouls[0]),
                iterator: nouls.iter(),
            },
            STATE,
        )
        .await
        .unwrap();

    let deep: &&&Noul = answers.deep;
    let list: Vec<&Noul> = answers.list;
    let option: Option<&Noul> = answers.option;
    let iterated: Vec<&Noul> = answers.iterator.collect();
    assert!(ptr::eq(*deep, borrowed_twice));
    assert_eq!(list.len(), 1);
    assert!(ptr::eq(list[0], borrowed));
    assert!(option.is_some_and(|noul| ptr::eq(noul, borrowed)));
    assert_eq!(iterated.len(), 1);
    assert!(ptr::eq(iterated[0], borrowed));
    assert!(no_request_sent(&server).await);
}

#[tokio::test]
async fn questions_keeps_a_borrowed_temporary_to_the_end_of_the_statement() {
    let server = mock_api(answer_set_response(&[
        &noul_answer(0.92),
        &noul_answer(0.3),
    ]))
    .await;

    let names = client_for(&server)
        .ask(
            questions! {
                buggy: BUGGY,
                name: &String::from("change"),
                style: questions! {
                    idiomatic: noul!("Is the code idiomatic?"),
                    name: &String::from("style"),
                },
            },
            STATE,
        )
        .await
        .map(|answers| (answers.name.clone(), answers.style.name.clone()))
        .unwrap();

    assert_eq!(names, ("change".to_owned(), "style".to_owned()));
    assert_eq!(instructions(&only_request(&server).await).len(), 2);
}

#[tokio::test]
async fn questions_asks_a_field_of_a_generic_question_set() {
    let server = mock_api(answer_set_response(&[&noul_answer(0.92)])).await;

    let (buggy, id) = ask_with_id(&client_for(&server), BUGGY).await.unwrap();

    assert_eq!(buggy.probability(), 0.92);
    assert_eq!(id, 7);
}

#[tokio::test]
async fn questions_keeps_the_names_of_the_caller() {
    // a binding of the expansion with one of these names would match the constant
    #[allow(non_upper_case_globals, reason = "the names of the bindings")]
    mod caller {
        pub(super) const field: u32 = 1;
        pub(super) const fields: u32 = 2;
        pub(super) const raw_question_set: u32 = 3;
        pub(super) const raw_answer_set: u32 = 4;
    }
    use caller::{field, fields, raw_answer_set, raw_question_set};

    let server = mock_api(answer_set_response(&[
        &noul_answer(0.92),
        &noul_answer(0.1),
        &json!({
            "type": "choice",
            "choice": "rust",
            "confidence": 0.7,
            "probabilities": { "rust": 0.7, "go": 0.3 },
        }),
    ]))
    .await;
    let id = 7;

    let answers = client_for(&server)
        .ask(
            questions! {
                id: BUGGY,
                number: id,
                review: Questions::<Review>::new(),
                constants: [field, fields, raw_question_set, raw_answer_set],
            },
            STATE,
        )
        .await
        .unwrap();

    assert_eq!(answers.id.probability(), 0.92);
    assert_eq!(answers.number, 7);
    assert_eq!(answers.review.buggy.probability(), 0.1);
    assert_eq!(answers.constants, [1, 2, 3, 4]);
}

#[tokio::test]
async fn questions_clones_a_question_set() {
    let server = mock_api(answer_set_response(&[&noul_answer(0.92)])).await;
    let client = client_for(&server);
    let question_set = questions! { buggy: BUGGY, id: 7 };

    let first = client.ask(question_set.clone(), STATE).await.unwrap();
    let second = client.ask(question_set, STATE).await.unwrap();

    assert_eq!(first.buggy.probability(), second.buggy.probability());
    assert_eq!(first.id, second.id);
}

#[tokio::test]
async fn questions_answers_serialize_clone_and_debug() {
    let server = mock_api(answer_set_response(&[&noul_answer(0.92)])).await;

    let answers = client_for(&server)
        .ask(questions! { buggy: BUGGY, id: 7 }, STATE)
        .await
        .unwrap();

    assert_eq!(
        serde_json::to_value(answers.clone()).unwrap(),
        json!({ "buggy": { "probability": 0.92 }, "id": 7 })
    );
    assert_eq!(
        format!("{answers:?}"),
        format!("Answers {{ buggy: {:?}, id: 7 }}", answers.buggy)
    );
}

#[tokio::test]
async fn questions_sends_no_request_for_no_question() {
    let server = mock_api(answer_set_response(&[])).await;
    let client = client_for(&server);

    let empty = client
        .ask(questions! { checks: Vec::<Noul>::new(), id: 7 }, STATE)
        .await
        .unwrap();
    let values = client
        .ask(questions! { id: 7, name: "change" }, STATE)
        .await
        .unwrap();
    let nothing = client.ask(questions! {}, STATE).await.unwrap();

    assert!(empty.checks.is_empty());
    assert_eq!(empty.id, 7);
    assert_eq!((values.id, values.name), (7, "change"));
    assert_eq!(serde_json::to_value(nothing).unwrap(), json!({}));
    assert!(no_request_sent(&server).await);
}
