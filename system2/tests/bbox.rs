mod support;

use serde_json::json;
use support::{
    STATE, answer_set_response, bbox_answer, client_for, mock_api, noul_answer, only_request, png,
    question_texts, request_body, response,
};
use system2::prelude::*;
use system2::{AskError, Bbox, Image, PixelBox};
use wiremock::ResponseTemplate;

const MUG: Bbox = bbox!("The mug.");
const MUG_ON_THE_DESK: &str = "The mug on the desk.";

#[tokio::test]
async fn bbox_sends_each_form_as_written() {
    let thing = "mug";
    let cases = [
        (MUG, r#"{"type":"box","instructions":"The mug."}"#),
        (
            bbox!(MUG_ON_THE_DESK),
            r#"{"type":"box","instructions":"The mug on the desk."}"#,
        ),
        (
            bbox!("The {thing} on the desk.",),
            r#"{"type":"box","instructions":"The mug on the desk."}"#,
        ),
        (
            bbox!({ "find": "the {thing}", "near": "the keyboard" }),
            r#"{"type":"box","instructions":{"find":"the mug","near":"the keyboard"}}"#,
        ),
        (
            Bbox::new(json!({ "find": "the mug" })).unwrap(),
            r#"{"type":"box","instructions":{"find":"the mug"}}"#,
        ),
    ];
    let (bboxes, texts): (Vec<_>, Vec<_>) = cases.into_iter().unzip();
    let answers: Vec<_> = texts
        .iter()
        .map(|_| bbox_answer(0.2, 0.1, 0.6, 0.5, 0.9))
        .collect();
    let server = mock_api(answer_set_response(&answers.iter().collect::<Vec<_>>())).await;

    client_for(&server).ask(bboxes, STATE).await.unwrap();

    assert_eq!(question_texts(&only_request(&server).await), texts);
}

#[tokio::test]
async fn ask_sends_the_images_header_with_a_bbox() {
    let server = mock_api(response(&bbox_answer(0.2, 0.1, 0.6, 0.5, 0.9))).await;

    client_for(&server).ask(MUG, STATE).await.unwrap();

    let request = only_request(&server).await;
    assert_eq!(request.headers["typesafe-danger-zone-images"], "true");
    assert_eq!(request.headers["content-type"], "application/json");
    assert!(request_body(&request).get("assets").is_none());
}

#[tokio::test]
async fn ask_reads_a_bbox_in_a_set() {
    let answers = [&bbox_answer(0.2, 0.1, 0.6, 0.5, 0.9), &noul_answer(0.95)];
    let server = mock_api(answer_set_response(&answers)).await;
    let question_set = questions! {
        mug: MUG.tag("mug"),
        present: noul!("Is there a mug?"),
    };

    let found = client_for(&server).ask(question_set, STATE).await.unwrap();

    let (mug, tag) = &found.mug;
    let edges = (mug.left(), mug.top(), mug.right(), mug.bottom());
    assert_eq!(
        (edges, mug.confidence(), *tag),
        ((0.2, 0.1, 0.6, 0.5), 0.9, "mug")
    );
    assert!(found.present.is_likely());
}

#[tokio::test]
async fn ask_refuses_a_bbox_on_an_older_model() {
    let invalid_request = ResponseTemplate::new(400).set_body_json(json!({
        "detail": { "error_type": "api_usage_error", "message": "Invalid request." },
    }));
    let server = mock_api(invalid_request).await;
    let client = client_for(&server).model("jev-1.13.0");

    let error = client.ask(MUG, STATE).await.unwrap_err();
    assert!(
        matches!(&error, AskError::BboxModel { model } if model == "jev-1.13.0"),
        "{error:?}"
    );

    // the same refusal of a request with no bbox has another cause
    let error = client
        .ask(noul!("Is there a mug?"), STATE)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AskError::Http { status: 400, message, .. } if message == "Invalid request."),
        "{error:?}"
    );
}

#[tokio::test]
async fn ask_refuses_a_box_with_its_edges_out_of_order() {
    let server = mock_api(response(&bbox_answer(0.7, 0.1, 0.6, 0.5, 0.9))).await;

    let error = client_for(&server).ask(MUG, STATE).await.unwrap_err();

    assert!(
        matches!(error, AskError::InvalidResponse { .. }),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "the response must give a left edge at most its right edge, not 0.7 and 0.6"
    );
}

#[tokio::test]
async fn to_pixels_gives_the_pixels_of_the_image() {
    let server = mock_api(response(&bbox_answer(0.25, 0.1, 0.75, 0.5, 0.9))).await;
    let photo = Image::new(png(400, 300)).unwrap();

    let found = client_for(&server).ask(MUG, &photo).await.unwrap();

    let pixel_box = PixelBox {
        left: 100,
        top: 30,
        right: 300,
        bottom: 150,
    };
    assert_eq!(found.to_pixels(&photo), pixel_box);
}
