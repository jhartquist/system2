mod support;

use std::env;
use support::{INSTRUCTIONS, STATE};
use system2::{AskError, Client, Noul};

#[tokio::test]
#[expect(unsafe_code, reason = "the test sets the environment")]
async fn from_env_ignores_empty_variables() {
    // SAFETY: this test is alone in its binary, and it starts no thread before this
    unsafe {
        env::set_var("TYPESAFE_API_KEY", "");
        env::set_var("TYPESAFE_BASE_URL", "");
        env::set_var("TYPESAFE_DEFAULT_MODEL", "");
    }

    let client = Client::from_env();

    assert_eq!(format!("{client:?}"), format!("{:?}", Client::new()));
    let error = client
        .ask(Noul::new(INSTRUCTIONS).unwrap(), STATE)
        .await
        .unwrap_err();
    assert!(matches!(error, AskError::MissingApiKey));
}
