//! System 2 decisions from System 1 judgments.

#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
#![cfg_attr(not(test), deny(clippy::float_cmp))]

mod ask;
mod client;
mod error;
mod image;
mod json;
mod question;

pub use ask::{
    Answers, AskError, AskFuture, AskIterator, AskWithReceiptFuture, QuestionSet, Questions,
    Receipt, Tagged, Usage,
};
pub use client::{CacheError, Client, Compression, EnvError};
pub use error::{Error, Result};
pub use image::{Image, InvalidImage};
pub use json::InvalidJson;
pub use question::{
    Bbox, BboxAnswer, Choice, ChoiceAnswer, ChoiceOption, InvalidBbox, InvalidChoice, InvalidNoul,
    InvalidScore, Label, Levels, Noul, NoulAnswer, Options, PixelBox, QuestionError, Score,
    ScoreAnswer, ScoreLevel,
};
pub use system2_macros::{Answers, Levels, Options, bbox, choice, noul, questions, score};

pub mod prelude {
    #[doc(no_inline)]
    pub use crate::{
        Answers, AskIterator, Levels, Options, QuestionSet, Questions, bbox, choice, image, noul,
        questions, score,
    };
}

#[doc(hidden)]
pub mod __private {
    pub use crate::ask::field::{
        DataKind, IteratorKind, Probe, QuestionSetKind, ReferenceKind, ResultKind, field,
    };
    pub use crate::ask::questions::{SealedAnswers, question_set};
    pub use crate::ask::wire::{RawAnswerSet, RawQuestionSet, Sealed};
    pub use crate::image::{image_error, static_image};
    pub use crate::json::{AlwaysSerialize, Json, is_blank, to_json, unescaped};
    pub use crate::question::bbox::bbox;
    pub use crate::question::choice::{choice, owned_choice};
    pub use crate::question::noul::noul;
    pub use crate::question::score::score;
    #[cfg(not(feature = "image"))]
    pub use core::include_bytes as include_image;
    pub use serde;
    #[cfg(feature = "image")]
    pub use system2_macros::include_image;
}
