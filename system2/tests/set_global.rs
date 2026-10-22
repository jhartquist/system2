mod support;

use support::{INSTRUCTIONS, STATE, client_for, mock_api, noul_response};
use system2::Noul;

#[tokio::test]
async fn set_global_replaces_the_client_of_a_later_ask() {
    let first = mock_api(noul_response(0.1)).await;
    let second = mock_api(noul_response(0.9)).await;

    client_for(&first).set_global();
    let early = Noul::new(INSTRUCTIONS).unwrap().ask(STATE);
    client_for(&second).set_global();
    let late = Noul::new(INSTRUCTIONS).unwrap().ask(STATE);

    assert!(early.await.unwrap().probability() <= 0.1);
    assert!(late.await.unwrap().probability() >= 0.9);
}
