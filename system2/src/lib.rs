//! System 2 decisions from System 1 judgments.

// library never prints, examples can
#![deny(clippy::print_stdout, clippy::print_stderr)]

mod error;
mod json;
mod question;

pub use error::InvalidJson;
pub use question::{InvalidNoul, Noul};
