#![allow(clippy::float_cmp, reason = "tests check exact API values")]

mod support;

use std::env;
use std::net::TcpListener;
use support::{INSTRUCTIONS, STATE, mount, noul_response, only_request, request_body};
use system2::Noul;
use wiremock::MockServer;

#[tokio::test]
#[expect(unsafe_code, reason = "the test sets the environment")]
async fn ask_uses_the_global_client_from_the_environment() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());

    // SAFETY: this test is alone in its binary, and it starts no thread before this
    unsafe {
        env::set_var("TYPESAFE_API_KEY", "env-key");
        env::set_var("TYPESAFE_BASE_URL", base_url);
        env::set_var("TYPESAFE_DEFAULT_MODEL", "jev-1.14.0");
    }
    let server = MockServer::builder().listener(listener).start().await;
    mount(&server, noul_response(0.92)).await;

    let answer = Noul::new(INSTRUCTIONS).unwrap().ask(STATE).await.unwrap();

    assert_eq!(answer.probability(), 0.92);
    let request = only_request(&server).await;
    assert_eq!(request.headers["authorization"], "Bearer env-key");
    assert_eq!(request_body(&request)["model"], "jev-1.14.0");
}
