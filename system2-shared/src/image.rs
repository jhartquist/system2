#[cfg(feature = "image")]
mod convert;
mod header;

#[cfg(feature = "image")]
pub use convert::{ConvertError, convert, encode};
pub use header::{Header, HeaderError, read_header};
