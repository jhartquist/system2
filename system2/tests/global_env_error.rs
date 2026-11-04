mod support;

use std::{env, fs};
use support::{INSTRUCTIONS, STATE, cache_path, noul};
use system2::prelude::*;
use system2::{AskError, CacheError, Client, EnvError, NoulAnswer};

#[expect(unsafe_code, reason = "the test sets the environment")]
fn set_var(name: &str, value: &str) {
    // SAFETY: this test is alone in its binary, and it starts no thread, as each ask fails
    // before a request
    unsafe { env::set_var(name, value) }
}

// the first `.ask` with one variable
async fn ask_with(name: &str, value: &str) -> Result<NoulAnswer, AskError> {
    set_var(name, value);
    let answer = noul(INSTRUCTIONS).ask(STATE).await;
    set_var(name, "");
    answer
}

#[tokio::test]
async fn ask_makes_the_global_client_again_after_a_bad_variable() {
    let folder =
        cache_path("ask_makes_the_global_client_again_after_a_bad_variable").with_extension("");
    let _ = fs::remove_dir_all(&folder);
    let path = folder.join("cache.jsonl");
    for name in ["TYPESAFE_API_KEY", "TYPESAFE_BASE_URL", "SYSTEM2_CACHE"] {
        set_var(name, "");
    }
    set_var("SYSTEM2_OFFLINE", "0");

    set_var("TYPESAFE_API_KEY", "first\nsecond");
    let global = Client::global().unwrap_err();
    set_var("TYPESAFE_API_KEY", "");

    assert!(matches!(global, EnvError::InvalidApiKey), "{global:?}");

    let key = ask_with("TYPESAFE_API_KEY", "first\nsecond")
        .await
        .unwrap_err();
    let url = ask_with("TYPESAFE_BASE_URL", "api.typesafe.ai")
        .await
        .unwrap_err();
    let cache = ask_with("SYSTEM2_CACHE", path.to_str().unwrap())
        .await
        .unwrap_err();

    assert!(matches!(key, AskError::InvalidApiKey), "{key:?}");
    assert!(matches!(url, AskError::InvalidBaseUrl { .. }), "{url:?}");
    assert!(
        matches!(cache, AskError::Cache(CacheError::Read { .. })),
        "{cache:?}"
    );

    // the cache opens now, and the client is online, so it needs a key
    fs::create_dir(&folder).unwrap();
    set_var("SYSTEM2_CACHE", path.to_str().unwrap());
    let online = noul(INSTRUCTIONS).ask(STATE).await.unwrap_err();

    assert!(matches!(online, AskError::MissingApiKey), "{online:?}");
    assert!(path.exists());

    // the global client stays, so a later bad variable does not change it
    set_var("TYPESAFE_API_KEY", "first\nsecond");
    let kept = noul(INSTRUCTIONS).ask(STATE).await.unwrap_err();

    assert!(matches!(kept, AskError::MissingApiKey), "{kept:?}");
}
