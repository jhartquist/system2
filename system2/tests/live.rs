//! Tests of the facts of the TypeSafe API. They replay offline from recorded responses.

// the answers replay offline from `live.jsonl`. to record them again, delete the file and run
// `SYSTEM2_RECORD=1 cargo test --test live -- --include-ignored` with `TYPESAFE_API_KEY`. the
// tests of openrouter need `OPENROUTER_API_KEY`

use serde::Serialize;
use serde_json::json;
use std::env;
use system2::prelude::*;
use system2::{AskError, Choice, Client, Image, PixelBox, Score};

const RECORDING: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/live.jsonl");

fn live() -> system2::Result<Client> {
    Ok(Client::from_env()?.model("jev-1.14.0"))
}

// offline unless asked, so a changed request fails, and calls no paid api
fn replay() -> system2::Result<Client> {
    let record = env::var("SYSTEM2_RECORD").as_deref() == Ok("1");
    Ok(live()?.offline(!record).cache(RECORDING)?)
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

// about 40k tokens, over the limit of the state and the longest question
const RED: Image = image!("images/red.png");

#[tokio::test]
async fn ask_sends_an_image_in_place_of_its_reference() -> system2::Result<()> {
    let red = replay()?
        .ask(noul!("Is the image mostly red?"), &RED)
        .await?;

    assert!(red.is_likely());
    Ok(())
}

// a black square from 320 to 440 px across and from 120 to 240 px down, on white
const SQUARE: Image = image!("images/square.png");

// a patch of the image, as the model sees it
const PATCH: u32 = 28;

fn assert_near_the_square(pixel_box: PixelBox, tolerance: u32) {
    let edges = [
        pixel_box.left,
        pixel_box.top,
        pixel_box.right,
        pixel_box.bottom,
    ];
    for (edge, expected) in edges.into_iter().zip([320, 120, 440, 240]) {
        assert!(edge.abs_diff(expected) <= tolerance, "{pixel_box:?}");
    }
}

#[tokio::test]
async fn ask_answers_a_bbox_with_the_edges_of_the_region() -> system2::Result<()> {
    let square = replay()?.ask(bbox!("The black square."), &SQUARE).await?;

    assert_near_the_square(square.to_pixels(&SQUARE), PATCH);
    Ok(())
}

#[tokio::test]
async fn ask_answers_a_bbox_in_the_image_with_the_region() -> system2::Result<()> {
    let state = json!({ "first": RED, "second": SQUARE });

    let square = replay()?.ask(bbox!("The black square."), &state).await?;

    // a second image makes the box less precise
    assert_near_the_square(square.to_pixels(&SQUARE), 2 * PATCH);
    Ok(())
}

#[tokio::test]
async fn ask_answers_a_bbox_on_an_image_in_the_instructions() -> system2::Result<()> {
    let bbox = bbox!({ "find": "The black square.", "image": SQUARE });

    let square = replay()?
        .ask(bbox, "Find the shape in the image of the question.")
        .await?;

    assert_near_the_square(square.to_pixels(&SQUARE), PATCH);
    Ok(())
}

#[tokio::test]
async fn ask_answers_a_bbox_with_no_image_at_a_low_confidence() -> system2::Result<()> {
    let state = "A kitchen table with a blue mug next to a laptop.";

    let mug = replay()?.ask(bbox!("The mug."), state).await?;

    assert!(mug.confidence() < 0.2, "{}", mug.confidence());
    Ok(())
}

fn over_the_default_limit() -> String {
    "word ".repeat(40_000)
}

#[tokio::test]
async fn long_context_raises_the_token_limit() -> system2::Result<()> {
    let long = replay()?
        .long_context(true)
        .ask(noul!("Is the text long?"), &over_the_default_limit())
        .await?;

    assert!(long.is_likely());
    Ok(())
}

// the cache keeps no refusals, and gives no tokens or request ID, so these need the api

#[tokio::test]
#[ignore = "needs TYPESAFE_API_KEY"]
async fn ask_with_receipt_tells_the_version_the_tokens_and_the_request_id() {
    let client = live().unwrap().model("jev-latest");

    let (_, receipt) = client
        .ask_with_receipt(noul!("Is this message spam?"), "Buy now!")
        .await
        .unwrap();

    let model = receipt.model.unwrap();
    assert!(model.starts_with("jev-1."), "{model}");

    // each request costs 259 tokens, plus the state and the question
    assert!(receipt.usage.input_tokens > 259, "{:?}", receipt.usage);
    assert!(receipt.request_id.is_some());
    assert_eq!(client.usage(), receipt.usage);
}

#[tokio::test]
#[ignore = "needs TYPESAFE_API_KEY"]
async fn ask_reports_the_token_limit() {
    let error = live()
        .unwrap()
        .ask(noul!("Is the text long?"), &over_the_default_limit())
        .await
        .unwrap_err();

    assert!(
        matches!(
            error,
            AskError::TokenLimit {
                long_context: false
            }
        ),
        "{error:?}"
    );
}

#[tokio::test]
#[ignore = "needs TYPESAFE_API_KEY"]
async fn ask_reports_the_token_limit_of_long_context() {
    // about 270k tokens, over the limit of long context
    let state = "word ".repeat(270_000);

    let error = live()
        .unwrap()
        .long_context(true)
        .ask(noul!("Is the text long?"), &state)
        .await
        .unwrap_err();

    assert!(
        matches!(error, AskError::TokenLimit { long_context: true }),
        "{error:?}"
    );
}

#[tokio::test]
#[ignore = "needs TYPESAFE_API_KEY"]
async fn ask_reports_a_bbox_on_an_older_model() {
    let error = live()
        .unwrap()
        .model("jev-1.13.0")
        .ask(bbox!("The black square."), &SQUARE)
        .await
        .unwrap_err();

    assert!(
        matches!(&error, AskError::BboxModel { model } if model == "jev-1.13.0"),
        "{error:?}"
    );
}

#[tokio::test]
#[ignore = "needs TYPESAFE_API_KEY"]
async fn ask_reports_an_unknown_model() {
    let error = live()
        .unwrap()
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

// openrouter takes its own key and model names, and no multipart body
fn openrouter() -> Client {
    let api_key = env::var("OPENROUTER_API_KEY").unwrap_or_default();
    Client::new()
        .base_url("https://openrouter.ai/api")
        .api_key(api_key)
}

#[tokio::test]
#[ignore = "needs OPENROUTER_API_KEY"]
async fn ask_answers_through_openrouter_with_a_receipt() {
    let (spam, receipt) = openrouter()
        .ask_with_receipt(noul!("Is this message spam?"), "Buy now!")
        .await
        .unwrap();

    assert!(spam.is_likely());
    let model = receipt.model.unwrap();
    assert!(model.starts_with("typesafe/jev-1."), "{model}");
    assert!(receipt.usage.input_tokens > 259, "{:?}", receipt.usage);
    let request_id = receipt.request_id.unwrap();
    assert!(request_id.starts_with("gen-"), "{request_id}");
}

#[tokio::test]
#[ignore = "needs OPENROUTER_API_KEY"]
async fn ask_sends_an_image_to_openrouter_in_the_json() {
    let square = openrouter()
        .ask(bbox!("The black square."), &SQUARE)
        .await
        .unwrap();

    assert_near_the_square(square.to_pixels(&SQUARE), PATCH);
}

#[tokio::test]
#[ignore = "needs OPENROUTER_API_KEY"]
async fn ask_reports_the_token_limit_of_openrouter() {
    // openrouter always has long context
    let state = "word ".repeat(270_000);

    let error = openrouter()
        .ask(noul!("Is the text long?"), &state)
        .await
        .unwrap_err();

    assert!(
        matches!(error, AskError::TokenLimit { long_context: true }),
        "{error:?}"
    );
}

#[tokio::test]
#[ignore = "needs OPENROUTER_API_KEY"]
async fn ask_reports_a_bbox_on_an_older_model_of_openrouter() {
    let error = openrouter()
        .model("jev-1.13")
        .ask(bbox!("The black square."), &SQUARE)
        .await
        .unwrap_err();

    assert!(
        matches!(&error, AskError::BboxModel { model } if model == "jev-1.13"),
        "{error:?}"
    );
}

#[tokio::test]
#[ignore = "needs OPENROUTER_API_KEY"]
async fn ask_reports_an_unknown_model_of_openrouter() {
    // the pinned names of typesafe have a patch number, and those of openrouter do not
    let error = openrouter()
        .model("jev-1.14.0")
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
    assert_eq!(
        (status, message.as_str()),
        (400, "Model typesafe/jev-1.14.0 does not exist")
    );
    assert!(request_id.is_some());
}
