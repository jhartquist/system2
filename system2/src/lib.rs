//! System 2 decisions from System 1 judgments.

#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
#![cfg_attr(not(test), deny(clippy::float_cmp))]

mod ask;
mod client;
mod error;
mod json;
mod question;

pub use ask::{Ask, AskError, Asking};
pub use client::Client;
pub use error::{Error, Result};
pub use json::InvalidJson;
pub use question::{
    Choice, ChoiceAnswer, ChoiceOption, InvalidChoice, InvalidNoul, InvalidScore, Label, Levels,
    Noul, NoulAnswer, Options, QuestionError, Score, ScoreAnswer, ScoreLevel,
};
pub use system2_macros::{Levels, Options};

pub mod prelude {
    pub use crate::{Ask, Levels, Options};
}
