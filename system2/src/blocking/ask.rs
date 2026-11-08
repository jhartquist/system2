use super::client::runtime;
use crate::{AskError, QuestionSet, Receipt, Tagged};
use serde::Serialize;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

static GLOBAL_RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub trait Ask: QuestionSet {
    fn ask<S: Serialize + ?Sized>(self, state: &S) -> Result<Self::AnswerSet, AskError> {
        global_runtime().block_on(crate::Ask::ask(self, state))
    }

    fn ask_with_receipt<S: Serialize + ?Sized>(
        self,
        state: &S,
    ) -> Result<(Self::AnswerSet, Receipt), AskError> {
        global_runtime().block_on(crate::Ask::ask_with_receipt(self, state))
    }

    fn tag<T>(self, tag: T) -> Tagged<Self, T> {
        Tagged::new(self, tag)
    }
}

impl<Q: QuestionSet> Ask for Q {}

type AnswerSets<I> = Vec<<<I as Iterator>::Item as QuestionSet>::AnswerSet>;

pub trait AskIterator: Iterator + Sized {
    fn ask<S: Serialize + ?Sized>(self, state: &S) -> Result<AnswerSets<Self>, AskError>
    where
        Self::Item: QuestionSet,
    {
        Ask::ask(self.collect::<Vec<_>>(), state)
    }

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
