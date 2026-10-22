use crate::{AskError, Noul, NoulAnswer};
use serde::Serialize;
use std::fmt;
use std::time::Duration;

#[derive(Clone)]
#[expect(dead_code, reason = "read when the methods have bodies")]
pub struct Client {
    http: reqwest::Client,
    api_key: Option<String>,
    base_url: String,
    model: String,
    timeout: Duration,
}

impl Client {
    #[must_use]
    pub fn new() -> Self {
        todo!()
    }

    #[must_use]
    pub fn api_key(self, api_key: impl Into<String>) -> Self {
        let _ = (self, api_key);
        todo!()
    }

    #[must_use]
    pub fn base_url(self, base_url: impl Into<String>) -> Self {
        let _ = (self, base_url);
        todo!()
    }

    #[must_use]
    pub fn model(self, model: impl Into<String>) -> Self {
        let _ = (self, model);
        todo!()
    }

    #[must_use]
    pub fn timeout(self, timeout: Duration) -> Self {
        let _ = (self, timeout);
        todo!()
    }

    pub async fn ask<S: Serialize + ?Sized>(
        &self,
        question: Noul,
        state: &S,
    ) -> Result<NoulAnswer, AskError> {
        let _ = (question, state);
        todo!()
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!()
    }
}
