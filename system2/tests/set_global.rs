//! Tests of `Client::set_global`.

mod support;

use support::{INSTRUCTIONS, STATE, client_for, mock_api, noul, noul_response};
use system2::Client;
use system2::prelude::*;

// the only test here, as the global client is one for each process
#[tokio::test]
async fn set_global_replaces_the_client_of_a_later_ask() {
    let first = mock_api(noul_response(0.1)).await;
    let second = mock_api(noul_response(0.9)).await;

    let first_client = client_for(&first).set_global();
    let early = noul(INSTRUCTIONS).ask(STATE);
    let early_global = Client::global().unwrap();
    let second_client = client_for(&second).set_global();
    let late = noul(INSTRUCTIONS).ask(STATE);
    let direct = second_client.ask(noul(INSTRUCTIONS), STATE);

    assert_eq!(early.await.unwrap().probability(), 0.1);
    assert_eq!(late.await.unwrap().probability(), 0.9);
    assert_eq!(direct.await.unwrap().probability(), 0.9);
    assert_eq!(first_client.usage().input_tokens, 280);
    assert_eq!(early_global.usage(), first_client.usage());
    assert_eq!(second_client.usage().input_tokens, 560);
    assert_eq!(Client::global().unwrap().usage(), second_client.usage());
}
