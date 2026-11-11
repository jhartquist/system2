//! Tests of `Image` and `image!` without the `image` feature.

// the images of one test file share a registry, so each test makes images of its own size

mod support;

use serde_json::{Value, json};
use support::{
    client_for, jpeg, mock_api, multipart, no_request_sent, noul_response, only_request, png,
};
use system2::{AskError, Image, image, noul};

const RED: Image = image!("images/red.png");
const RED_BYTES: &[u8] = include_bytes!("images/red.png");
const RED_REFERENCE: &str = "s2-img-272deed3263edbd6";

#[test]
fn new_reads_the_size_of_a_png_and_a_jpeg() {
    let png = Image::new(png(400, 300)).unwrap();
    let jpeg = Image::new(jpeg(640, 480)).unwrap();

    assert_eq!((png.width(), png.height()), (400, 300));
    assert_eq!((jpeg.width(), jpeg.height()), (640, 480));
}

#[test]
#[cfg(not(feature = "image"))]
fn new_refuses_an_invalid_image() {
    let mut too_large = png(300, 300);
    too_large.resize(16 * 1024 * 1024 + 1, 0);
    let cases = [
        (
            b"GIF89a".to_vec(),
            "the image must be PNG or JPEG: turn on the `image` feature of system2 to convert it",
        ),
        (
            b"\x89PNG\r\n\x1a\n".to_vec(),
            "the image must have a complete PNG or JPEG header",
        ),
        (
            png(279, 1568),
            "each side of the image must be from 280 to 1568 px, not 279x1568: \
             turn on the `image` feature of system2 to scale it",
        ),
        (
            too_large,
            "the image must have at most 16 MiB, not 16777217 bytes: \
             turn on the `image` feature of system2 to make it smaller",
        ),
        (
            support::png_with_orientation(400, 300, 6),
            "the image must be upright, but its EXIF orientation 6 turns or mirrors it, and the \
             API ignores the orientation: turn on the `image` feature of system2 to turn its \
             pixels",
        ),
    ];
    for (bytes, message) in cases {
        let error = Image::new(bytes).unwrap_err();

        assert_eq!(error.to_string(), message);
    }
}

#[test]
#[cfg(not(feature = "image"))]
fn new_takes_each_side_from_280_to_1568_px() {
    for (width, height) in [(280, 1568), (1568, 280)] {
        let image = Image::new(png(width, height)).unwrap();

        assert_eq!((image.width(), image.height()), (width, height));
    }
    let error = Image::new(png(1569, 280)).unwrap_err();
    assert!(matches!(
        error,
        system2::InvalidImage::Size {
            width: 1569,
            height: 280
        }
    ));
}

#[test]
fn image_reads_a_file_or_constant_bytes() {
    let from_bytes = image!(RED_BYTES);

    assert_eq!((RED.width(), RED.height()), (280, 280));
    assert_eq!((from_bytes.width(), from_bytes.height()), (280, 280));
}

#[test]
fn image_displays_and_serializes_as_its_reference() {
    let from_data = Image::new(RED_BYTES.to_vec()).unwrap();

    assert_eq!(RED.to_string(), RED_REFERENCE);
    assert_eq!(from_data.to_string(), RED_REFERENCE);
    assert_eq!(json!({ "logo": RED }), json!({ "logo": RED_REFERENCE }));
}

#[test]
fn image_debug_shows_the_size_and_not_the_bytes() {
    assert_eq!(
        format!("{RED:?}"),
        "Image { width: 280, height: 280, bytes: 861, \
         place: PixelBox { left: 0, top: 0, right: 280, bottom: 280 } }"
    );
}

#[tokio::test]
async fn ask_sends_each_image_in_a_part() {
    let server = mock_api(noul_response(0.9)).await;
    let logo = Image::new(png(500, 500)).unwrap();
    let question = noul!({ "question": "Is it the color of the logo?", "color": RED });
    let state = json!({ "logo": logo, "again": format!("The logo: {logo}") });

    client_for(&server).ask(question, &state).await.unwrap();

    let request = only_request(&server).await;
    let (content_type, parts) = multipart(&request);
    let json: Value = serde_json::from_slice(&parts[0].1).unwrap();
    let logo_reference = logo.to_string();
    let boundary = support::sha256_hex(std::str::from_utf8(&parts[0].1).unwrap());
    assert_eq!(
        content_type,
        format!("multipart/form-data; boundary={boundary}")
    );
    assert_eq!(request.headers["typesafe-danger-zone-images"], "true");
    assert_eq!(
        parts[0].0,
        "Content-Disposition: form-data; name=\"request\"\r\nContent-Type: application/json"
    );
    assert_eq!(
        json["assets"],
        json!({
            &logo_reference: { "part": &logo_reference },
            RED_REFERENCE: { "part": RED_REFERENCE },
        })
    );
    let image_part = |reference: &str| {
        format!("Content-Disposition: form-data; name=\"{reference}\"; filename=\"{reference}\"")
    };
    assert_eq!(parts.len(), 3);
    assert_eq!(
        parts[1],
        (image_part(RED_REFERENCE), RED_BYTES.to_vec()),
        "the parts are in the order of their references"
    );
    assert_eq!(parts[2], (image_part(&logo_reference), png(500, 500)));
}

#[tokio::test]
async fn ask_sends_an_image_in_a_placeholder_of_the_instructions() {
    let server = mock_api(noul_response(0.9)).await;
    let photo = Image::new(png(503, 503)).unwrap();

    client_for(&server)
        .ask(noul!("Is there a mug in {photo}?"), "A kitchen.")
        .await
        .unwrap();

    let (_, parts) = multipart(&only_request(&server).await);
    let json: Value = serde_json::from_slice(&parts[0].1).unwrap();
    let instructions = format!("Is there a mug in {photo}?");
    assert_eq!(json["questions"]["q0"]["instructions"], instructions);
    assert_eq!(parts[1].1, png(503, 503));
}

#[tokio::test]
async fn ask_sends_json_with_no_image() {
    let server = mock_api(noul_response(0.9)).await;

    client_for(&server)
        .ask(noul!("Is it red?"), "A red square.")
        .await
        .unwrap();

    let request = only_request(&server).await;
    assert_eq!(request.headers["content-type"], "application/json");
    assert!(!request.headers.contains_key("typesafe-danger-zone-images"));
    assert!(support::request_body(&request).get("assets").is_none());
}

#[tokio::test]
async fn ask_keeps_an_image_that_drops_after_ask() {
    let server = mock_api(noul_response(0.9)).await;
    let image = Image::new(png(501, 501)).unwrap();
    let state = format!("A photo: {image}");

    let answer = client_for(&server).ask(noul!("Is it a photo?"), &state);
    drop(image);
    answer.await.unwrap();

    let (_, parts) = multipart(&only_request(&server).await);
    assert_eq!(parts[1].1, png(501, 501));
}

#[tokio::test]
async fn ask_refuses_a_reference_with_no_image() {
    let server = mock_api(noul_response(0.9)).await;
    let image = Image::new(png(502, 502)).unwrap();
    let state = format!("A photo: {image}");
    let reference = image.to_string();
    drop(image);

    let error = client_for(&server)
        .ask(noul!("Is it a photo?"), &state)
        .await
        .unwrap_err();

    assert!(matches!(&error, AskError::MissingImage { reference: found } if *found == reference));
    assert_eq!(
        error.to_string(),
        format!(
            "the request refers to the image `{reference}`, and no `Image` holds it now: \
             keep the `Image` until `ask` returns"
        )
    );
    assert!(no_request_sent(&server).await);
}

#[tokio::test]
async fn ask_refuses_images_over_32_mib() {
    let server = mock_api(noul_response(0.9)).await;
    let images: Vec<Image> = (503..506)
        .map(|side| {
            let mut bytes = png(side, side);
            bytes.resize(11 * 1024 * 1024, 0);
            Image::new(bytes).unwrap()
        })
        .collect();
    let state = json!(images);

    let error = client_for(&server)
        .ask(noul!("Are these photos?"), &state)
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::ByteLimit { size: 34_603_008 }));
    assert_eq!(
        error.to_string(),
        "the images of the request must have at most 32 MiB, not 34603008 bytes: \
         split the request, or send smaller images"
    );
    assert!(no_request_sent(&server).await);
}

#[tokio::test]
async fn ask_ignores_a_reference_in_an_object_key() {
    let server = mock_api(noul_response(0.9)).await;
    let state = json!({ "s2-img-0123456789abcdef": "A key, not an image." });

    client_for(&server)
        .ask(noul!("Is it an image?"), &state)
        .await
        .unwrap();

    let request = only_request(&server).await;
    assert_eq!(request.headers["content-type"], "application/json");
}

#[tokio::test]
async fn ask_finds_an_image_in_the_text_of_a_question() {
    let server = mock_api(noul_response(0.9)).await;

    client_for(&server)
        .ask(noul!("Is {RED} the logo?"), "A logo.")
        .await
        .unwrap();

    let (_, parts) = multipart(&only_request(&server).await);
    assert_eq!(parts[1].1, RED_BYTES);
}
