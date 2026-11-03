// the answers replay offline from `live.jsonl`. to record them again, delete the file and run
// `SYSTEM2_RECORD=1 cargo test --test live -- --include-ignored` with `TYPESAFE_API_KEY`

use serde::Serialize;
use serde_json::json;
use std::env;
use system2::prelude::*;
use system2::{AskError, CacheError, Choice, Client, Score};

const RECORDING: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/live.jsonl");

fn live() -> Client {
    Client::from_env().model("jev-1.14.0")
}

// offline unless asked, so a changed request fails, and calls no paid api
fn replay() -> Result<Client, CacheError> {
    let record = env::var("SYSTEM2_RECORD").as_deref() == Ok("1");
    live().offline(!record).cache(RECORDING)
}

#[tokio::test]
async fn ask_answers_a_noul_in_each_shape() -> system2::Result<()> {
    let question_set = (
        noul!("Does the message name a day of the week?"),
        noul!("Does the message tell a price?", {
            "true": "The message gives an amount of money.",
        }),
        noul!("Is the meeting on Friday now?", {
            "true": "The meeting is on Friday.",
            "false": "The meeting is on a different day.",
        }),
    );

    let (day, price, friday) = replay()?
        .ask(
            question_set,
            "The meeting moved from Monday to Friday at 3 pm.",
        )
        .await?;

    let likely = [day, price, friday].map(|answer| answer.is_likely());
    assert_eq!(likely, [true, false, true]);
    Ok(())
}

const RUST_CODE: &str = "\
fn main() {
    let x: u32 = 1;
}
";

#[tokio::test]
async fn ask_answers_a_choice_by_option_name() -> system2::Result<()> {
    let language = Choice::with_descriptions(
        "Which language is the code in?",
        [
            ("python", Some("The code uses `def` and indentation.")),
            ("rust", None),
            ("go", None),
        ],
    )?;

    let language = replay()?.ask(language, RUST_CODE).await?;

    assert_eq!(*language.most_likely(), "rust");
    assert_eq!(language.probabilities().len(), 3);
    Ok(())
}

#[tokio::test]
async fn ask_answers_a_score_by_level_position() -> system2::Result<()> {
    let severity = Score::new(
        "How severe is the problem?",
        ["Nothing breaks.", "A feature breaks.", "All data is lost."],
    )?;

    let severity = replay()?
        .ask(
            severity,
            "The migration deleted each row of the customer table, and no backup exists.",
        )
        .await?;

    assert_eq!(*severity.most_likely(), "All data is lost.");
    Ok(())
}

const DIFF: &str = "\
-    n % 2 == 1
+    n % 2 == 0
";

#[tokio::test]
async fn ask_takes_json_in_each_slot() -> system2::Result<()> {
    #[derive(Serialize)]
    struct Change {
        diff: &'static str,
        description: &'static str,
    }

    let change = Change {
        diff: DIFF,
        description: "Fix `is_even`, which returned true for odd numbers.",
    };
    let described = noul!(
        {
            "question": "Does the description match the diff?",
            "rule": "The description tells what the diff changes.",
        },
        {
            "true": { "verdict": "It matches.", "because": "The diff makes the fix." },
            "false": { "verdict": "It does not match.", "because": "The diff does more." },
        }
    );
    let next = Choice::with_descriptions(
        "What must the reviewer do?",
        [
            (
                "approve",
                json!({ "action": "Approve.", "when": "The change is correct." }),
            ),
            (
                "block",
                json!({ "action": "Ask for a fix.", "when": "The change adds a bug." }),
            ),
        ],
    )?;
    let small = json!({ "size": "small", "lines": "1 to 5" });
    let large = json!({ "size": "large", "lines": "more than 100" });
    let size = Score::new("How big is the change?", [small.clone(), large])?;

    let (described, next, size) = replay()?.ask((described, next, size), &change).await?;

    assert!(described.is_likely());
    assert_eq!(*next.most_likely(), "approve");
    assert_eq!(*size.most_likely(), small);
    Ok(())
}

// the cache keeps no refusals, so these need the api

#[tokio::test]
#[ignore = "needs TYPESAFE_API_KEY"]
async fn ask_reports_the_token_limit() {
    // about 40k tokens, over the limit of the state and the longest question
    let state = "word ".repeat(40_000);

    let error = live()
        .ask(noul!("Is the text long?"), &state)
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::TokenLimit), "{error:?}");
}

#[tokio::test]
#[ignore = "needs TYPESAFE_API_KEY"]
async fn ask_reports_an_unknown_model() {
    let error = live()
        .model("jev-1.14")
        .ask(noul!("Is this message spam?"), "Buy now!")
        .await
        .unwrap_err();

    let AskError::Http {
        status,
        message,
        request_id,
    } = error
    else {
        panic!("{error:?}");
    };
    assert_eq!((status, message.as_str()), (400, "Unknown model: jev-1.14"));
    assert!(request_id.is_some());
}
