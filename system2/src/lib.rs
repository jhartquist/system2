//! System 2 decisions from System 1 judgments.

#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
#![cfg_attr(not(test), deny(clippy::float_cmp))]

mod ask;
mod client;
mod error;
mod json;
mod question;

pub use ask::{Answers, AskError, AskFuture, AskIterator, QuestionSet, Questions, Tagged};
pub use client::Client;
pub use error::{Error, Result};
pub use json::InvalidJson;
pub use question::{
    Choice, ChoiceAnswer, ChoiceOption, InvalidChoice, InvalidNoul, InvalidScore, Label, Levels,
    Noul, NoulAnswer, Options, QuestionError, Score, ScoreAnswer, ScoreLevel,
};
pub use system2_macros::{Answers, Levels, Options, choice, noul, questions, score};

pub mod prelude {
    #[doc(no_inline)]
    pub use crate::{
        Answers, AskIterator, Levels, Options, QuestionSet, Questions, choice, noul, questions,
        score,
    };
}

#[doc(hidden)]
pub mod __private {
    pub use crate::ask::field::{
        DataKind, IteratorKind, Probe, QuestionSetKind, ReferenceKind, ResultKind, field,
    };
    pub use crate::ask::questions::{SealedAnswers, question_set};
    pub use crate::ask::wire::{RawAnswerSet, RawQuestionSet, Sealed};
    pub use crate::json::{AlwaysSerialize, Json, is_blank, to_json, unescaped};
    pub use crate::question::choice::{choice, owned_choice};
    pub use crate::question::noul::noul;
    pub use crate::question::score::score;
    pub use serde;
}
