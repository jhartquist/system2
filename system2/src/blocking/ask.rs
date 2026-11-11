use super::client::runtime;
use crate::{AskError, QuestionSet, Receipt, Tagged};
use serde::Serialize;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

static GLOBAL_RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// The blocking `ask` of a question set, with the global client.
///
/// Each [`QuestionSet`] implements this trait. [`Client::ask`](super::Client::ask) asks with a
/// given client.
pub trait Ask: QuestionSet {
    /// Asks the set about a state, with the global client, and waits for the answers.
    ///
    /// The state follows the rules of the async [`Ask::ask`](crate::Ask::ask). The global
    /// client sends the request on a runtime of its own, which the process keeps.
    ///
    /// # Errors
    ///
    /// Returns an [`AskError`] as the async [`Ask::ask`](crate::Ask::ask) does.
    ///
    /// # Panics
    ///
    /// Panics if it runs in an async runtime.
    fn ask<S: Serialize + ?Sized>(self, state: &S) -> Result<Self::AnswerSet, AskError> {
        global_runtime().block_on(crate::Ask::ask(self, state))
    }

    /// Asks the set about a state, and returns a [`Receipt`] next to the answers.
    ///
    /// # Errors
    ///
    /// Returns an [`AskError`] as the async [`Ask::ask`](crate::Ask::ask) does.
    ///
    /// # Panics
    ///
    /// Panics if it runs in an async runtime.
    fn ask_with_receipt<S: Serialize + ?Sized>(
        self,
        state: &S,
    ) -> Result<(Self::AnswerSet, Receipt), AskError> {
        global_runtime().block_on(crate::Ask::ask_with_receipt(self, state))
    }

    /// Puts a tag next to the set, so that the answers come back next to it.
    ///
    /// It is the same as the async [`Ask::tag`](crate::Ask::tag).
    fn tag<T>(self, tag: T) -> Tagged<Self, T> {
        Tagged::new(self, tag)
    }
}

impl<Q: QuestionSet> Ask for Q {}

type AnswerSets<I> = Vec<<<I as Iterator>::Item as QuestionSet>::AnswerSet>;

/// The blocking `ask` of an iterator of question sets.
///
/// An iterator is not a [`QuestionSet`], so this trait asks its sets as a `Vec`.
pub trait AskIterator: Iterator + Sized {
    /// Asks each set of the iterator in one request, with the global client.
    ///
    /// # Errors
    ///
    /// Returns an [`AskError`] as the async [`Ask::ask`](crate::Ask::ask) does.
    ///
    /// # Panics
    ///
    /// Panics if it runs in an async runtime.
    fn ask<S: Serialize + ?Sized>(self, state: &S) -> Result<AnswerSets<Self>, AskError>
    where
        Self::Item: QuestionSet,
    {
        Ask::ask(self.collect::<Vec<_>>(), state)
    }

    /// Asks each set of the iterator, and returns a [`Receipt`] next to the answers.
    ///
    /// # Errors
    ///
    /// Returns an [`AskError`] as the async [`Ask::ask`](crate::Ask::ask) does.
    ///
    /// # Panics
    ///
    /// Panics if it runs in an async runtime.
    fn ask_with_receipt<S: Serialize + ?Sized>(
        self,
        state: &S,
    ) -> Result<(AnswerSets<Self>, Receipt), AskError>
    where
        Self::Item: QuestionSet,
    {
        Ask::ask_with_receipt(self.collect::<Vec<_>>(), state)
    }
}

impl<I: Iterator> AskIterator for I {}

fn global_runtime() -> &'static Runtime {
    GLOBAL_RUNTIME.get_or_init(runtime)
}
