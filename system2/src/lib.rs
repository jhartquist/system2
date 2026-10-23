//! System 2 decisions from System 1 judgments.

#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
#![cfg_attr(not(test), deny(clippy::float_cmp))]

mod ask;
mod client;
mod error;
mod json;
mod question;

pub use ask::{Ask, Asking};
pub use client::Client;
pub use error::{AskError, InvalidJson};
pub use question::{
    Choice, ChoiceAnswer, ChoiceProbability, InvalidChoice, InvalidNoul, InvalidScore, Label, Noul,
    NoulAnswer, Score, ScoreAnswer, ScoreProbability,
};

pub mod prelude {
    pub use crate::Ask;
}
