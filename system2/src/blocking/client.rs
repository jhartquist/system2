use crate::{AskError, QuestionSet, Receipt, Usage};
use serde::Serialize;
use std::fmt;
use std::sync::Arc;
use tokio::runtime::{self, Runtime};

#[derive(Clone)]
pub struct Client {
    client: crate::Client,
    runtime: Arc<Runtime>,
}

impl Client {
    pub fn ask<Q: QuestionSet, S: Serialize + ?Sized>(
        &self,
        question_set: Q,
        state: &S,
    ) -> Result<Q::AnswerSet, AskError> {
        self.runtime.block_on(self.client.ask(question_set, state))
    }

    pub fn ask_with_receipt<Q: QuestionSet, S: Serialize + ?Sized>(
        &self,
        question_set: Q,
        state: &S,
    ) -> Result<(Q::AnswerSet, Receipt), AskError> {
        self.runtime
            .block_on(self.client.ask_with_receipt(question_set, state))
    }

    #[must_use]
    pub fn usage(&self) -> Usage {
        self.client.usage()
    }
}

impl From<crate::Client> for Client {
    fn from(client: crate::Client) -> Self {
        let runtime = Arc::new(runtime());
        Client { client, runtime }
    }
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("client", &self.client)
            .finish_non_exhaustive()
    }
}

// the worker runs each connection while no request runs, as clients of other runtimes can share
// the connections
#[expect(
    clippy::expect_used,
    reason = "a runtime fails to start only when the system has no threads or files left"
)]
pub(super) fn runtime() -> Runtime {
    runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("the runtime starts")
}

#[cfg(test)]
mod tests {
    use super::Client;

    #[test]
    fn debug_shows_the_client_that_it_runs() {
        let client = Client::from(crate::Client::new());

        let debug = format!("{client:?}");

        assert!(
            debug.starts_with("Client { client: Client { base_url: "),
            "{debug}"
        );
        assert!(debug.ends_with(", .. }, .. }"), "{debug}");
    }
}
