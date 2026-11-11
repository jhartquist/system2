//! Tests of `Client::from_env` with a cache file and offline mode.

mod support;

use std::{env, fs};
use support::{
    INSTRUCTIONS, STATE, answer_set_body, cache_path, noul, noul_answer, noul_cache_line,
};
use system2::{AskError, Client};

#[tokio::test]
#[expect(unsafe_code, reason = "the test sets the environment")]
async fn from_env_reads_the_cache_and_offline_mode() {
    let path = cache_path("from_env_reads_the_cache_and_offline_mode");
    let response = answer_set_body(&[&noul_answer(0.92)]);
    fs::write(&path, noul_cache_line(STATE, &response)).unwrap();

    // SAFETY: this test is alone in its binary, and it starts no thread before this
    unsafe {
        env::set_var("TYPESAFE_API_KEY", "");
        env::set_var("TYPESAFE_BASE_URL", "");
        env::set_var("TYPESAFE_DEFAULT_MODEL", "");
        env::set_var("SYSTEM2_CACHE", &path);
        env::set_var("SYSTEM2_OFFLINE", "1");
    }

    let client = Client::from_env().unwrap();

    let answer = client.ask(noul(INSTRUCTIONS), STATE).await.unwrap();
    let error = client.ask(noul(INSTRUCTIONS), "Hello").await.unwrap_err();
    assert_eq!(answer.probability(), 0.92);
    assert!(matches!(error, AskError::Offline), "{error:?}");
}
