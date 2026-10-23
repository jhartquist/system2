//! System 2 decisions from System 1 judgments.

#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
#![cfg_attr(
    test,
    allow(clippy::float_cmp, reason = "tests check exact API values")
)]

mod ask;
mod client;
mod error;
mod json;
mod question;

pub use ask::Asking;
pub use client::Client;
pub use error::{AskError, InvalidJson};
pub use question::{InvalidNoul, Noul, NoulAnswer};
