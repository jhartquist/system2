mod registry;

use crate::json::AlwaysSerialize;
use serde::{Serialize, Serializer};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;
#[cfg(feature = "image")]
use system2_shared::image::ConvertError;
use system2_shared::image::{Header, HeaderError, read_header};

pub(crate) use registry::find;

const REFERENCE_PREFIX: &str = "s2-img-";
const REFERENCE_HEX_DIGITS: usize = 16;

#[derive(Clone)]
pub struct Image {
    width: u32,
    height: u32,
    bytes: Bytes,
}

impl Image {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, InvalidImage> {
        let bytes = bytes.into();
        #[cfg(feature = "image")]
        let bytes = system2_shared::image::convert(&bytes)?.unwrap_or(bytes);
        let Header { width, height } = read_header(&bytes)?;
        let reference = reference_of(&bytes);
        let shared = registry::register(Shared { reference, bytes })?;
        let bytes = Bytes::Shared(shared);
        Ok(Image {
            width,
            height,
            bytes,
        })
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    // a constant cannot hash its file, so a file of `image!` joins the registry when it is written
    fn reference(&self) -> String {
        match &self.bytes {
            Bytes::Static(bytes) => {
                let reference = reference_of(bytes);
                registry::register_static(&reference, bytes);
                reference
            }
            Bytes::Shared(shared) => shared.reference.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidImage {
    #[error("the image must be PNG or JPEG: turn on the `image` feature of system2 to convert it")]
    Format,
    #[error("the image must have a complete PNG or JPEG header")]
    Header,
    #[cfg_attr(
        not(feature = "image"),
        error(
            "each side of the image must be from 280 to 1568 px, not {width}x{height}: \
             turn on the `image` feature of system2 to scale it"
        )
    )]
    #[cfg_attr(
        feature = "image",
        error(
            "the long side of the image must be at most 5.6 times its short side, \
             not {width}x{height}: crop it"
        )
    )]
    Size { width: u32, height: u32 },
    #[error(
        "the image must have at most 16 MiB, not {size} bytes: \
         turn on the `image` feature of system2 to make it smaller"
    )]
    Bytes { size: usize },
    #[error(
        "the image must be upright, but its EXIF orientation {orientation} turns or mirrors it, \
         and the API ignores the orientation: turn on the `image` feature of system2 to turn its \
         pixels"
    )]
    Orientation { orientation: u16 },
    #[error(
        "each image must have a different reference, but a different image has the reference \
         `{reference}`: change a byte of one of them"
    )]
    Reference { reference: String },
    #[cfg(feature = "image")]
    #[error("the image must be a valid PNG, JPEG, WebP, or GIF")]
    Decode(#[from] ::image::ImageError),
}

// the bytes of a file from `image!`, or of an image from run time
#[derive(Clone)]
pub(crate) enum Bytes {
    Static(&'static [u8]),
    Shared(Arc<Shared>),
}

impl Bytes {
    pub(crate) fn as_slice(&self) -> &[u8] {
        match self {
            Bytes::Static(bytes) => bytes,
            Bytes::Shared(shared) => &shared.bytes,
        }
    }
}

// the bytes of an image from run time, in the registry while an `Image` holds them
pub(crate) struct Shared {
    reference: String,
    bytes: Vec<u8>,
}

impl Drop for Shared {
    fn drop(&mut self) {
        registry::forget(&self.reference);
    }
}

// the references in the text values of these json texts. the api keeps a reference in a key as
// text, such as in the name of an option
pub(crate) fn references<'a>(json_texts: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    let mut references = BTreeSet::new();
    for json_text in json_texts {
        if !json_text.contains(REFERENCE_PREFIX) {
            continue;
        }
        if let Ok(value) = serde_json::from_str(json_text) {
            add_references(&value, &mut references);
        }
    }
    references
}

fn add_references(value: &Value, references: &mut BTreeSet<String>) {
    match value {
        Value::String(text) => {
            for (start, _) in text.match_indices(REFERENCE_PREFIX) {
                let end = start + REFERENCE_PREFIX.len() + REFERENCE_HEX_DIGITS;
                let Some(reference) = text.get(start..end) else {
                    continue;
                };
                let digits = &reference[REFERENCE_PREFIX.len()..];
                if digits
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
                {
                    references.insert(reference.to_owned());
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                add_references(item, references);
            }
        }
        Value::Object(fields) => {
            for field in fields.values() {
                add_references(field, references);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn reference_of(bytes: &[u8]) -> String {
    let sha256 = Sha256::digest(bytes);
    let hex_digits = hex::encode(&sha256[..REFERENCE_HEX_DIGITS / 2]);
    format!("{REFERENCE_PREFIX}{hex_digits}")
}

// the message of `image!`, which panics in its own expansion, so that rustc shows the error at
// the call and not in this crate
#[doc(hidden)]
#[must_use]
pub const fn image_error(bytes: &[u8]) -> Option<&'static str> {
    match read_header(bytes) {
        Ok(_) => None,
        Err(error) => Some(error.message()),
    }
}

#[doc(hidden)]
#[must_use]
pub const fn static_image(bytes: &'static [u8]) -> Image {
    match read_header(bytes) {
        Ok(Header { width, height }) => {
            let bytes = Bytes::Static(bytes);
            Image {
                width,
                height,
                bytes,
            }
        }
        Err(error) => panic!("{}", error.message()),
    }
}

#[macro_export]
macro_rules! image {
    ($path:literal) => {
        $crate::image!($crate::__private::include_image!($path))
    };

    // the length of an array is a constant, so `cargo check` validates the image
    ($bytes:expr) => {
        $crate::__private::static_image(
            [const { $bytes }; {
                if let ::core::option::Option::Some(message) =
                    $crate::__private::image_error($bytes)
                {
                    ::core::panic!("{}", message);
                }
                1
            }][0],
        )
    };
}

impl From<HeaderError> for InvalidImage {
    fn from(error: HeaderError) -> Self {
        match error {
            HeaderError::Format => InvalidImage::Format,
            HeaderError::Header => InvalidImage::Header,
            HeaderError::Size { width, height } => InvalidImage::Size { width, height },
            HeaderError::Bytes { size } => InvalidImage::Bytes { size },
            HeaderError::Orientation { orientation } => InvalidImage::Orientation { orientation },
        }
    }
}

#[cfg(feature = "image")]
impl From<ConvertError> for InvalidImage {
    fn from(error: ConvertError) -> Self {
        match error {
            ConvertError::Decode(error) => InvalidImage::Decode(error),
            ConvertError::Size { width, height } => InvalidImage::Size { width, height },
        }
    }
}

#[cfg(feature = "image")]
impl TryFrom<::image::DynamicImage> for Image {
    type Error = InvalidImage;

    fn try_from(dynamic_image: ::image::DynamicImage) -> Result<Self, Self::Error> {
        let bytes = system2_shared::image::encode(&dynamic_image, None)?;
        Image::new(bytes)
    }
}

#[cfg(feature = "image")]
impl TryFrom<&Image> for ::image::DynamicImage {
    type Error = InvalidImage;

    fn try_from(image: &Image) -> Result<Self, Self::Error> {
        Ok(::image::load_from_memory(image.bytes.as_slice())?)
    }
}

impl fmt::Display for Image {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reference())
    }
}

// the bytes can have megabytes, so it shows only their number
impl fmt::Debug for Image {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.bytes.as_slice().len())
            .finish()
    }
}

impl Serialize for Image {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.reference())
    }
}

impl AlwaysSerialize for Image {}

#[cfg(test)]
mod tests {
    use super::{HeaderError, Image, InvalidImage, image_error, references, static_image};
    use std::collections::BTreeSet;

    #[test]
    fn references_finds_references_in_text_values_only() {
        let first = "s2-img-0123456789abcdef";
        let second = "s2-img-fedcba9876543210";
        let state = serde_json::json!({
            "text": format!("{first} and {first}0, but not s2-img-0123"),
            "list": [second, 42, null, "s2-img-0123456789ABCDEF"],
            "s2-img-1111111111111111": "a key",
        })
        .to_string();
        let question = r#"{"type":"noul","instructions":"s2-img-2222222222222222"}"#;

        let found = references([state.as_str(), question, r#""no reference""#]);

        let expected = [first, second, "s2-img-2222222222222222"];
        assert_eq!(found, BTreeSet::from(expected.map(str::to_owned)));
    }

    #[test]
    fn from_keeps_the_values_of_a_header_error() {
        let format = InvalidImage::from(HeaderError::Format);
        let header = InvalidImage::from(HeaderError::Header);
        let size = InvalidImage::from(HeaderError::Size {
            width: 279,
            height: 300,
        });
        let bytes = InvalidImage::from(HeaderError::Bytes { size: 17 });
        let orientation = InvalidImage::from(HeaderError::Orientation { orientation: 6 });

        assert!(matches!(format, InvalidImage::Format));
        assert!(matches!(header, InvalidImage::Header));
        assert!(matches!(
            size,
            InvalidImage::Size {
                width: 279,
                height: 300
            }
        ));
        assert!(matches!(bytes, InvalidImage::Bytes { size: 17 }));
        assert!(matches!(
            orientation,
            InvalidImage::Orientation { orientation: 6 }
        ));
    }

    #[test]
    fn image_error_gives_the_message_of_an_invalid_image() {
        let red = include_bytes!("../tests/images/red.png");

        assert_eq!(image_error(red), None);
        assert_eq!(
            image_error(b"\x89PNG\r\n\x1a\n"),
            Some("the image must have a complete PNG or JPEG header")
        );
    }

    #[test]
    fn static_image_reads_the_size() {
        let image: Image = static_image(include_bytes!("../tests/images/red.png"));

        assert_eq!((image.width(), image.height()), (280, 280));
    }

    #[test]
    #[should_panic(expected = "the image must be PNG or JPEG")]
    fn static_image_panics_on_an_invalid_image() {
        let _ = static_image(b"GIF89a");
    }
}
