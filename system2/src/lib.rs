//! System 2 decisions from System 1 judgments.

// library never prints, examples can
#![deny(clippy::print_stdout, clippy::print_stderr)]

mod ask;
mod client;
mod error;
mod json;
mod question;

pub use ask::Asking;
pub use client::Client;
pub use error::{AskError, InvalidJson};
pub use question::{InvalidNoul, Noul, NoulAnswer};
