//! The same `ask`, with no `async`.
//!
//! The blocking [`Ask`] and [`AskIterator`] have the names and the methods of the async traits.
//! They return the answers, not a future. Both modes share [`QuestionSet`](crate::QuestionSet).
//! Thus a program changes one import, from `system2::prelude` to `system2::blocking::prelude`, and
//! removes each `.await`.
//!
//! `.ask` uses the global client, as the async `.ask` does. [`Client`] asks with a given client.
//! Each blocking client has a Tokio runtime with one worker thread. The global client has one
//! runtime for the process.
//!
//! A blocking `ask` in an async runtime panics, and so does a drop of a blocking client there.
//! Tokio cannot start or stop a runtime in a runtime.
//!
//! # Examples
//!
//! ```no_run
//! use system2::blocking::prelude::*;
//!
//! fn main() -> system2::Result<()> {
//!     let answer = noul!("Is the ticket urgent?").ask("The site is down.")?;
//!     println!("{:.2}", answer.probability());
//!     Ok(())
//! }
//! ```

mod ask;
mod client;

pub use ask::{Ask, AskIterator};
pub use client::Client;

/// The prelude of the blocking API.
///
/// It is [`system2::prelude`](crate::prelude), with the blocking [`Ask`] and [`AskIterator`] in
/// place of the async traits.
pub mod prelude {
    #[doc(no_inline)]
    pub use super::{Ask, AskIterator};
    #[doc(no_inline)]
    pub use crate::{
        Answers, Levels, Options, QuestionSet, Questions, bbox, choice, image, noul, questions,
        score,
    };
}
