mod support;

use serde_json::json;
use std::cell::Cell;
use support::{
    Counted, STATE, answer_set_response, client_for, counted_drops, mock_api, noul, noul_answer,
    only_request, request_text,
};
use system2::prelude::*;
use system2::{
    Choice, ChoiceAnswer, NamedAnswerSet, Noul, NoulAnswer, Questions, Score, ScoreAnswer,
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
