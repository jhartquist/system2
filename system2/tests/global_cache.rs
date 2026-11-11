//! Tests of the global client with the cache of the environment.

mod support;

use std::{env, fs};
use support::{
    INSTRUCTIONS, STATE, answer_set_body, cache_path, noul, noul_answer, noul_cache_line,
};
use system2::AskError;
use system2::prelude::*;

#[tokio::test]
#[expect(unsafe_code, reason = "the test sets the environment")]
async fn ask_uses_the_cache_of_the_environment_offline() {
    let path = cache_path("ask_uses_the_cache_of_the_environment_offline");
    let response = answer_set_body(&[&noul_answer(0.92)]);
    fs::write(&path, noul_cache_line(STATE, &response)).unwrap();

    // SAFETY: this test is alone in its binary, and it starts no thread before this
    unsafe {
        env::set_var("TYPESAFE_API_KEY", "");
        env::set_var("TYPESAFE_DEFAULT_MODEL", "");
        env::set_var("SYSTEM2_CACHE", format!(" {}\n", path.display()));
        env::set_var("SYSTEM2_OFFLINE", " 1\n");
    }

    let answer = noul(INSTRUCTIONS).ask(STATE).await.unwrap();
    let error = noul(INSTRUCTIONS).ask("Hello").await.unwrap_err();

    assert_eq!(answer.probability(), 0.92);
    assert!(matches!(error, AskError::Offline), "{error:?}");
}
