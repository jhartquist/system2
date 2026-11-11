//! A Rust client for TypeSafe's System One model, [Jev].
//!
//! Daniel Kahneman's *Thinking, Fast and Slow* describes two modes of thought: System 1 is fast
//! and intuitive, System 2 is slow and deliberate. Jev is a [System One] model from [TypeSafe].
//! You give it some input (the state), such as a support ticket or a screenshot, ask it a narrow
//! question, and get back a fast answer with calibrated probabilities. `system2` is how you build
//! the System 2 part on top: ask as many small questions as you need, then combine the judgments
//! in plain Rust.
//!
//! Each question is a Rust type. The ones you write with a macro are checked by the compiler, so
//! a bad question never costs a request. Each answer is typed too, and keeps its probabilities,
//! so the thresholds are yours to pick, and to change later without asking again. The
//! [guide](https://github.com/jhartquist/system2/blob/main/system2/GUIDE.md) walks through the
//! crate in order.
//!
//! [Jev]: https://typesafe.ai/blog/introducing-system-one-models-and-jev
//! [TypeSafe]: https://typesafe.ai/
//! [System One]: https://docs.typesafe.ai/concepts/system-one
//!
//! # Quick start
//!
//! Add the crate and Tokio, and set `TYPESAFE_API_KEY` to your key from [TypeSafe]:
//!
//! ```sh
//! cargo add system2
//! cargo add tokio --features macros,rt-multi-thread
//! export TYPESAFE_API_KEY=...
//! ```
//!
//! Then ask a question about a state, and decide from the answer:
//!
//! ```no_run
//! use system2::prelude::*;
//!
//! const CODE: &str = "\
//! fn is_even(n: u32) -> bool {
//!     n % 2 == 1
//! }";
//!
//! #[tokio::main]
//! async fn main() -> system2::Result<()> {
//!     let question = noul!("Is there a bug in this code?");
//!     let answer = question.ask(CODE).await?;
//!     println!("{:.2}", answer.probability()); // 0.95
//!     if answer.is_likely() { /* do something */ }
//!     Ok(())
//! }
//! ```
//!
//! # The main types
//!
//! - Questions. A question asks Jev for one judgment about a state. There are four kinds:
//!   - [`Noul`] asks yes or no. Its [`NoulAnswer`] gives you the probability of yes.
//!   - [`Choice`] picks one option from a list. Its [`ChoiceAnswer`] gives you the probability of
//!     each option.
//!   - [`Score`] picks a level on a scale, from lowest to highest. Its [`ScoreAnswer`] gives you
//!     the mean level and the probability of each level.
//!   - [`Bbox`] finds a region of an image, such as a button in a screenshot. Its
//!     [`BboxAnswer`] gives you the edges of the region.
//!
//!   Write a question with a macro, such as [`noul!`], and the compiler checks it. Build one
//!   from data with a constructor, such as [`Noul::new`].
//! - Question sets. One request can ask a whole [`QuestionSet`] at once:
//!   - A tuple, an array, or a `Vec` of questions.
//!   - [`questions!`], which gives each question a name.
//!   - [`Questions`], which asks the questions of a struct that derives [`Answers`].
//! - [`Client`] holds the API key, the model, the cache, and the other settings of a request.
//!   Calling `ask` on a question uses the global client, which is whatever you passed to
//!   [`Client::set_global`], or else one built from the environment on the first `ask`.
//! - [`Image`] is an image you can put in a state, a question, or an option. With the `image`
//!   feature, the crate converts images to fit the limits of the API.
//! - [`Receipt`] has the model, the tokens, and the time of one request.
//! - [`Error`] covers every error in the crate, so `?` works on all of them. [`AskError`] tells
//!   you why a request failed.
//!
//! # No async
//!
//! [`blocking`] has the same API with no `.await`. Import `system2::blocking::prelude::*`
//! instead of `system2::prelude::*`.
//!
//! # Recordings
//!
//! Give a client a cache file, and it records every valid response and replays it for an equal
//! request. `SYSTEM2_CACHE` sets the file, and `SYSTEM2_OFFLINE=1` makes the client answer only
//! from it, so tests and examples run with no key and no network. See [`Client::cache`] and
//! [`Client::offline`].

#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
#![cfg_attr(not(test), deny(clippy::float_cmp))]

mod ask;
pub mod blocking;
mod client;
mod error;
mod image;
mod json;
mod question;

pub use ask::{
    Answers, Ask, AskError, AskFuture, AskIterator, AskWithReceiptFuture, QuestionSet, Questions,
    Receipt, Tagged, Usage,
};
pub use client::{CacheError, Client, Compression, EnvError};
pub use error::{Error, Result};
pub use image::{Image, InvalidImage};
pub use json::InvalidJson;
pub use question::{
    Bbox, BboxAnswer, Choice, ChoiceAnswer, InvalidAnswer, InvalidBbox, InvalidChoice, InvalidNoul,
    InvalidScore, Label, Levels, Noul, NoulAnswer, Options, PixelBox, QuestionError, Score,
    ScoreAnswer,
};
pub use system2_macros::{Answers, Levels, Options, bbox, choice, noul, questions, score};

/// The traits and the macros that each program uses.
///
/// `use system2::prelude::*` imports [`QuestionSet`], [`Ask`], and [`AskIterator`] for `.ask`,
/// the question macros, [`Questions`], and the derives with their traits.
pub mod prelude {
    #[doc(no_inline)]
    pub use crate::{
        Answers, Ask, AskIterator, Levels, Options, QuestionSet, Questions, bbox, choice, image,
        noul, questions, score,
    };
}

#[doc(hidden)]
pub mod __private {
    pub use crate::ask::field::{
        DataKind, IteratorKind, Probe, QuestionSetKind, ReferenceKind, ResultKind, field,
    };
    pub use crate::ask::questions::{SealedAnswers, answers, question_set};
    pub use crate::ask::wire::{RawAnswerSet, RawQuestionSet, Sealed};
    #[cfg(feature = "image")]
    pub use crate::image::converted_image;
    pub use crate::image::{image_error, static_image};
    pub use crate::json::{AlwaysSerialize, Json, is_blank, to_json, unescaped};
    pub use crate::question::bbox::bbox;
    pub use crate::question::choice::{ChoiceOption, choice, owned_choice};
    pub use crate::question::noul::noul;
    pub use crate::question::score::{ScoreLevel, score};
    pub use serde;
    pub use system2_macros::include_image;
}
