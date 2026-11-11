use crate::{AskError, QuestionSet, Receipt, Usage};
use serde::Serialize;
use std::fmt;
use std::sync::Arc;
use tokio::runtime::{self, Handle, Runtime};

/// A client that waits for the answers.
///
/// `Client::from` takes a [`system2::Client`](crate::Client) with its settings. The builder
/// methods are only on that client. The blocking client shares its connections, its cache, its
/// limit on requests, and its total of tokens.
///
/// Each blocking client has a Tokio runtime with one worker thread, and its clones share it. A
/// drop of the last clone in an async runtime panics, because Tokio cannot stop a runtime there.
///
/// # Examples
///
/// ```no_run
/// use system2::{Client, blocking, noul};
///
/// fn main() -> system2::Result<()> {
///     let client = blocking::Client::from(Client::from_env()?.model("jev-1.14.0"));
///     let urgent = noul!("Is the ticket urgent?");
///     let (answer, receipt) = client.ask_with_receipt(urgent, "The site is down.")?;
///     println!("{:.2}, {} tokens in", answer.probability(), receipt.usage.input_tokens);
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct Client {
    client: crate::Client,
    runtime: Arc<OwnedRuntime>,
}

// a runtime panics on a drop in async code, but not on a shutdown in the background
struct OwnedRuntime {
    handle: Handle,
    runtime: Option<Runtime>,
}

impl Drop for OwnedRuntime {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl Client {
    /// Asks a question set about a state, with this client, and waits for the answers.
    ///
    /// # Errors
    ///
    /// Returns an [`AskError`] as [`crate::Client::ask`] does.
    ///
    /// # Panics
    ///
    /// Panics if it runs in an async runtime.
    pub fn ask<Q: QuestionSet, S: Serialize + ?Sized>(
        &self,
        question_set: Q,
        state: &S,
    ) -> Result<Q::AnswerSet, AskError> {
        self.runtime
            .handle
            .block_on(self.client.ask(question_set, state))
    }

    /// Asks a question set with this client, and returns a [`Receipt`] next to the answers.
    ///
    /// # Errors
    ///
    /// Returns an [`AskError`] as [`crate::Client::ask`] does.
    ///
    /// # Panics
    ///
    /// Panics if it runs in an async runtime.
    pub fn ask_with_receipt<Q: QuestionSet, S: Serialize + ?Sized>(
        &self,
        question_set: Q,
        state: &S,
    ) -> Result<(Q::AnswerSet, Receipt), AskError> {
        self.runtime
            .handle
            .block_on(self.client.ask_with_receipt(question_set, state))
    }

    /// Returns the total of tokens of the client, its clones, and the client that it took.
    #[must_use]
    pub fn usage(&self) -> Usage {
        self.client.usage()
    }
}

impl From<crate::Client> for Client {
    fn from(client: crate::Client) -> Self {
        let runtime = runtime();
        let handle = runtime.handle().clone();
        let runtime = Arc::new(OwnedRuntime {
            handle,
            runtime: Some(runtime),
        });
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
