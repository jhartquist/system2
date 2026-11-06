#[cfg(feature = "image")]
mod convert;
mod header;

#[cfg(feature = "image")]
pub use convert::{ConvertError, Converted, convert, decode, encode, fitted_size};
pub use header::{Header, HeaderError, read_header};
