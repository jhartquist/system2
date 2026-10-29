mod support;

use serde_json::{Value, json};
use std::cmp::Ordering;
use std::rc::Rc;
use support::{
    Counted, STATE, answer_set_response, client_for, counted_drops, mock_api, only_request,
    question_texts, request_text, response,
};
use system2::prelude::*;
use system2::{AskError, Choice, ChoiceAnswer};
use wiremock::ResponseTemplate;

const REQUEST_ID: &str = "req_01a13d1fbca4728f8f7c1cdd312bd3a5";

#[derive(Debug)]
struct File {
    path: String,
}

#[derive(Debug, PartialEq, Options)]
enum Problem {
    /// The code returns the wrong value
    /// for some input.
    ///
    /// It does not panic.
    WrongResult,
    /// The code panics for some input.
    #[system2(rename = "panic")]
    Crash,
    /// The code is slow, but this text is for rustdoc only.
    #[system2(description = "")]
    Slow,
    Style,
    /// The code is correct.
    #[system2(rename = "no_problem", description = "The code has no problem.")]
    None,
}

const PROBLEM: Choice<Problem> = choice!("What is the worst problem in this code?", Problem);

const NEXT: Choice<&str> = choice!("What must the reviewer do?", {
    "approve": "Approve the change.",
    "block": "Ask for a fix.",
});

const LANGUAGE: Choice<&str> = choice!("Which language is the code in?", ["rust", "python", "go"]);

const BETTER: Choice<Ordering> = choice!(
    "Which answer is better?",
    [
        ("first", "The first answer is better.", Ordering::Less),
        ("second", "The second answer is better.", Ordering::Greater),
        ("same", "The two answers are equally good.", Ordering::Equal),
    ]
);

const BLOCK_TEXT: &str = "Ask for a fix.";

fn choice_answer(choice: &str, names: &[&str]) -> Value {
    let probabilities: serde_json::Map<String, Value> = names
        .iter()
        .map(|&name| {
            let probability = if name == choice { 1.0 } else { 0.0 };
            (name.to_owned(), json!(probability))
        })
        .collect();
    json!({
        "type": "choice",
        "choice": choice,
        "confidence": 1.0,
        "probabilities": probabilities,
    })
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
async fn ask_sends_the_options_of_the_derive_and_gives_back_a_value() {
    let server = mock_api(response(&json!({
        "type": "choice",
        "choice": "no_problem",
        "confidence": 0.9,
        "probabilities": {
            "wrong_result": 0.02,
            "panic": 0.02,
            "slow": 0.02,
            "style": 0.02,
            "no_problem": 0.92,
        },
    })))
    .await;
    let question = Choice::<Problem>::of("What is the worst problem?").unwrap();

    let problem = client_for(&server)
        .ask(question, STATE)
        .await
        .unwrap()
        .into_most_likely();

    assert_eq!(problem, Problem::None);
    let request = only_request(&server).await;
    assert_eq!(
        request_text(&request),
        concat!(
            r#"{"model":"jev-latest","state":"Buy now!","questions":{"q0":{"type":"choice","#,
            r#""instructions":"What is the worst problem?","criteria":{"#,
            r#""wrong_result":"The code returns the wrong value for some input.\n\nIt does not panic.","#,
            r#""panic":"The code panics for some input.","slow":null,"style":null,"#,
            r#""no_problem":"The code has no problem."}}}}"#,
        )
    );
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
            "the response must choose an option of the Choice, not `src/main.rs` \
             (request ID `{REQUEST_ID}`)"
        )
    );
    let AskError::InvalidResponse { request_id, .. } = error else {
        panic!("expected an invalid response, got {error:?}");
    };
    assert_eq!(request_id.as_deref(), Some(REQUEST_ID));
}

#[tokio::test]
async fn choice_sends_each_form_as_written() {
    let library = "serde";
    let rule = json!({ "tests": "pass" });
    let cases = [
        (
            NEXT,
            choice_answer("block", &["approve", "block"]),
            "block",
            r#"{"type":"choice","instructions":"What must the reviewer do?","criteria":{"approve":"Approve the change.","block":"Ask for a fix."}}"#,
        ),
        (
            LANGUAGE,
            choice_answer("go", &["rust", "python", "go"]),
            "go",
            r#"{"type":"choice","instructions":"Which language is the code in?","criteria":{"rust":null,"python":null,"go":null}}"#,
        ),
        (
            choice!("Which `{library}` call?", ["a{{b}}", ("c", "d")]),
            choice_answer("a{b}", &["a{b}", "c"]),
            "a{b}",
            r#"{"type":"choice","instructions":"Which `serde` call?","criteria":{"a{b}":null,"c":null}}"#,
        ),
        (
            choice!("What next?", [
                ("block", BLOCK_TEXT, "no"),
                ("approve", { "when": "tests pass" }, "yes"),
            ]),
            choice_answer("block", &["block", "approve"]),
            "no",
            r#"{"type":"choice","instructions":"What next?","criteria":{"block":"Ask for a fix.","approve":{"when":"tests pass"}}}"#,
        ),
        (
            choice!("What next?", {
                "approve": "Use `{library}`.",
                "block": { "unless": rule },
            }),
            choice_answer("block", &["approve", "block"]),
            "block",
            r#"{"type":"choice","instructions":"What next?","criteria":{"approve":"Use `serde`.","block":{"unless":{"tests":"pass"}}}}"#,
        ),
    ];
    let answers: Vec<_> = cases.iter().map(|(_, answer, _, _)| answer).collect();
    let server = mock_api(answer_set_response(&answers)).await;
    let values: Vec<_> = cases.iter().map(|&(_, _, value, _)| value).collect();
    let texts: Vec<_> = cases.iter().map(|&(_, _, _, text)| text).collect();
    let choices: Vec<_> = cases.into_iter().map(|(choice, ..)| choice).collect();

    let answers = client_for(&server).ask(choices, STATE).await.unwrap();

    let answer_values: Vec<_> = answers
        .into_iter()
        .map(ChoiceAnswer::into_most_likely)
        .collect();
    assert_eq!(answer_values, values);
    assert_eq!(question_texts(&only_request(&server).await), texts);
}

#[tokio::test]
async fn choice_gives_back_the_values_of_a_type_or_a_tuple() {
    let function = "parse";
    let problems = ["wrong_result", "panic", "slow", "style", "no_problem"];
    let answers = [
        choice_answer("panic", &problems),
        choice_answer("second", &["first", "second", "same"]),
        choice_answer("style", &problems),
    ];
    let server = mock_api(answer_set_response(&answers.iter().collect::<Vec<_>>())).await;

    let (problem, better, in_function) = client_for(&server)
        .ask(
            (
                PROBLEM,
                BETTER,
                choice!("What is the worst problem in `{function}`?", Problem),
            ),
            STATE,
        )
        .await
        .unwrap();

    match problem.most_likely() {
        Problem::Crash => {}
        other => panic!("expected a crash, got {other:?}"),
    }
    assert_eq!(problem.probabilities().len(), 5);
    assert_eq!(
        better.into_probabilities(),
        [
            (Ordering::Less, 0.0),
            (Ordering::Greater, 1.0),
            (Ordering::Equal, 0.0)
        ]
    );
    assert_eq!(in_function.into_most_likely(), Problem::Style);
    let texts = question_texts(&only_request(&server).await);
    assert_eq!(
        texts[0],
        concat!(
            r#"{"type":"choice","instructions":"What is the worst problem in this code?","#,
            r#""criteria":{"#,
            r#""wrong_result":"The code returns the wrong value for some input.\n\nIt does not panic.","#,
            r#""panic":"The code panics for some input.","slow":null,"style":null,"#,
            r#""no_problem":"The code has no problem."}}"#,
        )
    );
    assert_eq!(
        texts[1],
        r#"{"type":"choice","instructions":"Which answer is better?","criteria":{"first":"The first answer is better.","second":"The second answer is better.","same":"The two answers are equally good."}}"#
    );
    assert!(texts[2].contains(r#""instructions":"What is the worst problem in `parse`?""#));
}

#[tokio::test]
async fn choice_takes_a_value_that_starts_with_a_bracket_or_a_brace() {
    let server = mock_api(response(&choice_answer("second", &["first", "second"]))).await;
    let question: Choice<u32> = choice!(
        "Which one?",
        [("first", [10, 20][0]), ("second", { (40, 0) }.0)]
    );

    let answer = client_for(&server).ask(question, STATE).await.unwrap();

    assert_eq!(answer.into_probabilities(), [(10, 0.0), (40, 1.0)]);
}

#[tokio::test]
async fn choice_keeps_each_value_until_the_answer_drops() {
    let server = mock_api(response(&choice_answer("second", &["first", "second"]))).await;
    let question = choice!("Which one?", [("first", Counted), ("second", Counted)]);

    let ask_future = client_for(&server).ask(question, STATE);
    assert_eq!(counted_drops(), 0);
    let answer = ask_future.await.unwrap();
    assert_eq!(counted_drops(), 0);

    drop(answer);
    assert_eq!(counted_drops(), 2);
}

#[derive(Debug, PartialEq, Options)]
enum V2 {
    Yes,
    No,
}

fn choice_of<O: Options>() -> Choice<O> {
    choice!("Which one?", O)
}

#[tokio::test]
async fn choice_takes_each_path_to_a_type() {
    let problems = ["wrong_result", "panic", "slow", "style", "no_problem"];
    let answers = [
        choice_answer("no", &["yes", "no"]),
        choice_answer("panic", &problems),
        choice_answer("panic", &problems),
        choice_answer("panic", &problems),
    ];
    let server = mock_api(answer_set_response(&answers.iter().collect::<Vec<_>>())).await;

    let (version, raw, qualified, generic) = client_for(&server)
        .ask(
            (
                choice!("Which one?", V2),
                choice!("Which one?", r#Problem),
                choice!("Which one?", crate::Problem),
                choice_of::<Problem>(),
            ),
            STATE,
        )
        .await
        .unwrap();

    assert_eq!(version.into_most_likely(), V2::No);
    for problem in [raw, qualified, generic] {
        assert_eq!(problem.into_most_likely(), Problem::Crash);
    }
}

trait Limit {
    const MAX: u64;
}

struct Big;

impl Limit for Big {
    const MAX: u64 = 5_000_000_000;
}

fn choice_up_to<T: Limit>() -> Choice<u64> {
    choice!("How many?", [("some", 1), ("most", T::MAX)])
}

#[tokio::test]
async fn choice_infers_each_value_from_the_question() {
    const MAYBE: Choice<Option<u8>> = choice!("How many?", [("none", None), ("one", Some(1))]);
    const MANY: Choice<u64> = choice!("How many?", [("many", 5_000_000_000)]);
    let answers = [
        choice_answer("none", &["none", "one"]),
        choice_answer("many", &["many"]),
        choice_answer("most", &["some", "most"]),
    ];
    let server = mock_api(answer_set_response(&answers.iter().collect::<Vec<_>>())).await;

    let (maybe, many, generic) = client_for(&server)
        .ask((MAYBE, MANY, choice_up_to::<Big>()), STATE)
        .await
        .unwrap();

    assert_eq!(maybe.into_most_likely(), None);
    assert_eq!(many.into_most_likely(), 5_000_000_000);
    assert_eq!(generic.into_most_likely(), 5_000_000_000);
}
