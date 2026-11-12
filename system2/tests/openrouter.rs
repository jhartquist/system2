// the mock stands in for openrouter as a proxy, so the client sees the host of openrouter

mod support;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::json;
use std::env;
use std::net::TcpListener;
use support::{jpeg, mount, noul_response, only_request, request_body};
use system2::prelude::*;
use system2::{Client, Image};
use wiremock::MockServer;

const RED: Image = image!("images/red.png");
const RED_BYTES: &[u8] = include_bytes!("images/red.png");
const RED_REFERENCE: &str = "s2-img-272deed3263edbd6";

#[tokio::test]
#[expect(unsafe_code, reason = "the test sets the environment")]
async fn ask_sends_openrouter_each_image_in_the_json() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());

    // SAFETY: this test is alone in its binary, and it starts no thread before this
    unsafe {
        env::set_var("HTTP_PROXY", proxy);
    }

    let server = MockServer::builder().listener(listener).start().await;
    mount(&server, noul_response(0.97)).await;
    let client = Client::new()
        .api_key("key")
        .base_url("http://openrouter.ai");

    let photo_bytes = jpeg(300, 300);
    let photo = Image::new(photo_bytes.clone()).unwrap();
    let state = json!({ "chart": RED, "photo": photo });

    client
        .ask(noul!("Is the chart in the photo?"), &state)
        .await
        .unwrap();

    let request = only_request(&server).await;
    assert_eq!(request.url.host_str(), Some("openrouter.ai"));
    assert_eq!(request.headers["content-type"], "application/json");
    let chart =
        json!({ "type": "image", "format": "png", "data_base64": BASE64.encode(RED_BYTES) });
    let photo_asset =
        json!({ "type": "image", "format": "jpeg", "data_base64": BASE64.encode(photo_bytes) });
    assert_eq!(
        request_body(&request)["assets"],
        json!({ RED_REFERENCE: chart, photo.to_string(): photo_asset })
    );
}
