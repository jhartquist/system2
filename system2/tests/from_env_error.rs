mod support;

use std::env;
use support::cache_path;
use system2::{CacheError, Client, EnvError, Error};

#[expect(unsafe_code, reason = "the test sets the environment")]
fn set_var(name: &str, value: &str) {
    // SAFETY: this test is alone in its binary, and it starts no thread
    unsafe { env::set_var(name, value) }
}

// `from_env` with one variable, through `?` as a caller uses it
fn from_env_with(name: &str, value: &str) -> system2::Result<Client> {
    set_var(name, value);
    let client = Client::from_env();
    set_var(name, "");
    Ok(client?)
}

#[test]
fn from_env_refuses_a_bad_variable() {
    let missing_folder = cache_path("from_env_refuses_a_bad_variable").with_extension("");
    let path = missing_folder.join("cache.jsonl");
    for name in ["TYPESAFE_API_KEY", "TYPESAFE_BASE_URL", "SYSTEM2_CACHE"] {
        set_var(name, "");
    }

    let key = from_env_with("TYPESAFE_API_KEY", "first\nsecond").unwrap_err();
    let url = from_env_with("TYPESAFE_BASE_URL", "api.typesafe.ai").unwrap_err();
    let cache = from_env_with("SYSTEM2_CACHE", path.to_str().unwrap()).unwrap_err();

    assert!(
        matches!(key, Error::Env(EnvError::InvalidApiKey)),
        "{key:?}"
    );
    assert!(
        matches!(url, Error::Env(EnvError::InvalidBaseUrl { .. })),
        "{url:?}"
    );
    assert!(
        matches!(cache, Error::Env(EnvError::Cache(CacheError::Read { .. }))),
        "{cache:?}"
    );
    assert_eq!(
        key.to_string(),
        "`TYPESAFE_API_KEY` must be a valid HTTP header value: \
         remove line breaks and other control characters"
    );
    assert_eq!(
        url.to_string(),
        "`TYPESAFE_BASE_URL` must be an absolute HTTP or HTTPS URL, not `api.typesafe.ai`"
    );

    // the same rule as the builder method
    let builder = Client::new().cache(&path).unwrap_err();
    assert_eq!(cache.to_string(), builder.to_string());
}
