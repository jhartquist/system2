mod support;

use std::{env, fs};
use support::{INSTRUCTIONS, STATE, cache_path, noul};
use system2::prelude::*;
use system2::{AskError, CacheError};

#[tokio::test]
#[expect(unsafe_code, reason = "the test sets the environment")]
async fn ask_opens_the_cache_of_the_environment_again_after_an_error() {
    let folder = cache_path("ask_opens_the_cache_of_the_environment_again_after_an_error")
        .with_extension("");
    let _ = fs::remove_dir_all(&folder);
    let path = folder.join("cache.jsonl");

    // SAFETY: this test is alone in its binary, and it starts no thread before this
    unsafe {
        env::set_var("TYPESAFE_API_KEY", "");
        env::set_var("SYSTEM2_CACHE", &path);
        env::set_var("SYSTEM2_OFFLINE", "0");
    }

    let error = noul(INSTRUCTIONS).ask(STATE).await.unwrap_err();
    fs::create_dir(&folder).unwrap();
    let online = noul(INSTRUCTIONS).ask(STATE).await.unwrap_err();

    assert!(
        matches!(error, AskError::Cache(CacheError::Read { .. })),
        "{error:?}"
    );
    let prefix = format!(
        "the client cannot read the cache file `{}`: ",
        path.display()
    );
    assert!(error.to_string().starts_with(&prefix), "{error}");

    // the cache opens now, and the client is online, so it needs a key
    assert!(matches!(online, AskError::MissingApiKey), "{online:?}");
    assert!(path.exists());
}
