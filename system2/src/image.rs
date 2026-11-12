mod registry;

use crate::PixelBox;
use crate::json::AlwaysSerialize;
use serde::{Serialize, Serializer};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;
use system2_shared::image::{Header, HeaderError, read_header};
#[cfg(feature = "image")]
use {
    ::image::{DynamicImage, ImageFormat},
    std::borrow::Cow,
    system2_shared::image::{ConvertError, Converted, decode, encode, fitted_size},
};

pub(crate) use registry::find;

const REFERENCE_PREFIX: &str = "s2-img-";
const REFERENCE_HEX_DIGITS: usize = 16;

/// A PNG or JPEG image that goes where text goes.
///
/// An image goes in a state, a question, or an option, through `format!`, `json!`, or a struct.
/// It displays and serializes as its reference: `s2-img-` and 16 hex digits of its SHA-256.
/// `ask` finds each reference in the text values of the request, and sends the bytes of its
/// image. A reference in an object key, such as the name of an option, stays text. A clone shares
/// the bytes.
///
/// [`image!`](crate::image) makes an image that the compiler checks. [`Image::new`] makes one
/// from data. With the `image` feature, `TryFrom` converts an `image::DynamicImage` into an
/// image, and an `&Image` into a `DynamicImage`.
///
/// An image keeps its original: the image that the caller gave, upright. With the `image`
/// feature, `DynamicImage::try_from(&image)` gives the place of the image in its original, with
/// all of its detail.
///
/// # Examples
///
/// ```no_run
/// use serde_json::json;
/// use system2::prelude::*;
/// use system2::Image;
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let screenshot = Image::new(std::fs::read("screenshot.png")?)?;
/// let state = json!({ "screenshot": screenshot, "message": "The app shows an error." });
/// let answer = noul!("Does the screenshot show the error?").ask(&state).await?;
/// let answer = noul!("Is there a dialog in {screenshot}?").ask("A screenshot.").await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Image {
    width: u32,
    height: u32,
    bytes: Bytes,
    place: PixelBox,
    #[cfg(feature = "image")]
    original: Original,
}

impl Image {
    /// Makes an image from bytes, and validates it.
    ///
    /// Without the `image` feature, the image must be PNG or JPEG, with no EXIF orientation that
    /// turns or mirrors it. Each side must be 280 to 1568 px, and the image must have 16 MiB or
    /// less. An image that passes keeps its bytes, with no copy.
    ///
    /// The `image` feature also takes WebP, and the first frame of a GIF, of each size and
    /// orientation. It turns the pixels upright, and scales the image into the limits. A JPEG
    /// stays a JPEG, and each other format becomes a PNG. An image that passes without the
    /// feature does not change.
    ///
    /// # Errors
    ///
    /// - [`InvalidImage::Format`] if the image is not PNG or JPEG, without the `image` feature.
    /// - [`InvalidImage::Header`] if the PNG or JPEG header is not complete, without the `image`
    ///   feature.
    /// - [`InvalidImage::Size`] if a side is out of the limits, without the `image` feature. With
    ///   the feature, if the long side is more than 5.6 times the short side.
    /// - [`InvalidImage::Bytes`] if the image has more than 16 MiB, without the `image` feature.
    /// - [`InvalidImage::Orientation`] if the EXIF orientation turns or mirrors the image, without
    ///   the `image` feature.
    /// - [`InvalidImage::Reference`] if a different image has the same reference.
    /// - `InvalidImage::Decode` if the image needs a conversion, and does not decode, with the
    ///   `image` feature. Also if its pixels need more than 512 MiB of memory. An image that
    ///   passes without the feature is not decoded.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, InvalidImage> {
        let bytes = bytes.into();
        #[cfg(feature = "image")]
        if let Some(Converted {
            bytes: converted,
            original: Header { width, height },
        }) = system2_shared::image::convert(&bytes)?
        {
            let place = whole(width, height);
            let original = Original::Converted(Arc::new(bytes));
            return Image::with_original(converted, place, original);
        }
        let (Header { width, height }, bytes) = register(bytes)?;
        let place = whole(width, height);
        #[cfg(feature = "image")]
        let original = Original::Encoded(bytes.clone());
        Ok(Image {
            width,
            height,
            bytes,
            place,
            #[cfg(feature = "image")]
            original,
        })
    }

    #[cfg(feature = "image")]
    fn with_original(
        bytes: Vec<u8>,
        place: PixelBox,
        original: Original,
    ) -> Result<Self, InvalidImage> {
        let (Header { width, height }, bytes) = register(bytes)?;
        Ok(Image {
            width,
            height,
            bytes,
            place,
            original,
        })
    }

    /// Cuts a region from the original of the image, with all of its detail.
    ///
    /// The region is in the pixels of the original, as [`BboxAnswer::to_pixels`] gives it. The
    /// crop is a new image, which `Image::new` converts, and it shares the original. A
    /// [`BboxAnswer`] on the crop also gives the pixels of the original. `crop` decodes the
    /// original each time.
    ///
    /// # Errors
    ///
    /// - `InvalidImage::Region` if the region is not in the original, or has no area. Also if
    ///   its long side is more than 5.6 times its short side.
    /// - `InvalidImage::Decode` if the original does not decode, or the crop does not encode.
    ///   Also if the pixels of the original need more than 512 MiB of memory.
    ///
    /// [`BboxAnswer`]: crate::BboxAnswer
    /// [`BboxAnswer::to_pixels`]: crate::BboxAnswer::to_pixels
    #[cfg(feature = "image")]
    pub fn crop(&self, region: PixelBox) -> Result<Image, InvalidImage> {
        let (original, format) = self.original.decode()?;
        let (width, height) = (original.width(), original.height());
        let PixelBox {
            left,
            top,
            right,
            bottom,
        } = region;
        let inside = right <= width && bottom <= height;

        // a region with no area has no size that fits
        let (cut_width, cut_height) = (right.saturating_sub(left), bottom.saturating_sub(top));
        let fits = fitted_size(cut_width, cut_height).is_some();
        if !(inside && fits) {
            return Err(InvalidImage::Region {
                region,
                width,
                height,
            });
        }
        let cut = original.crop_imm(left, top, cut_width, cut_height);
        let bytes = encode(&cut, format)?;
        Image::with_original(bytes, region, self.original.clone())
    }

    /// Returns the width that the API sees, in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Returns the height that the API sees, in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    pub(crate) fn place(&self) -> PixelBox {
        self.place
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

/// An image that breaks a rule of [`Image::new`], or a region that `Image::crop` refuses.
///
/// Without the `image` feature, the message of each rule that the feature fixes names the
/// feature as the fix.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidImage {
    /// The image is not PNG or JPEG.
    #[error("the image must be PNG or JPEG: turn on the `image` feature of system2 to convert it")]
    Format,
    /// The PNG or JPEG header is not complete.
    #[error("the image must have a complete PNG or JPEG header")]
    Header,
    /// A side is out of the limits of 280 to 1568 px. With the `image` feature, the long side is
    /// more than 5.6 times the short side, and no scale fits.
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
    Size {
        /// The width, in pixels.
        width: u32,
        /// The height, in pixels.
        height: u32,
    },
    /// The image has more than 16 MiB.
    #[error(
        "the image must have at most 16 MiB, not {size} bytes: \
         turn on the `image` feature of system2 to make it smaller"
    )]
    Bytes {
        /// The size of the image, in bytes.
        size: usize,
    },
    /// The EXIF orientation turns or mirrors the image. The API ignores the orientation.
    #[error(
        "the image must be upright, but its EXIF orientation {orientation} turns or mirrors it, \
         and the API ignores the orientation: turn on the `image` feature of system2 to turn its \
         pixels"
    )]
    Orientation {
        /// The value of the EXIF orientation, from 2 to 8.
        orientation: u16,
    },
    /// A different image has the same reference.
    #[error(
        "each image must have a different reference, but a different image has the reference \
         `{reference}`: change a byte of one of them"
    )]
    Reference {
        /// The reference of the two images.
        reference: String,
    },
    /// The image does not decode, its pixels need more than 512 MiB of memory, or a crop does
    /// not encode.
    #[cfg(feature = "image")]
    #[error("the image must be a valid PNG, JPEG, WebP, or GIF")]
    Decode(#[from] ::image::ImageError),
    /// The region of a crop is not in the original, has no area, or does not fit.
    #[cfg(feature = "image")]
    #[error(
        "the region must be in the original of {width}x{height} px, have an area, and have a long \
         side at most 5.6 times its short side, not ({}, {}) to ({}, {})",
        region.left,
        region.top,
        region.right,
        region.bottom
    )]
    Region {
        /// The region of the crop.
        region: PixelBox,
        /// The width of the original, in pixels.
        width: u32,
        /// The height of the original, in pixels.
        height: u32,
    },
}

// reads the image that the model sees, and keeps it in the registry while an `Image` holds it
fn register(bytes: Vec<u8>) -> Result<(Header, Bytes), InvalidImage> {
    let header = read_header(&bytes)?;
    let reference = reference_of(&bytes);
    let shared = registry::register(Shared { reference, bytes })?;
    Ok((header, Bytes::Shared(shared)))
}

const fn whole(width: u32, height: u32) -> PixelBox {
    PixelBox {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    }
}

// the image that the caller gave, with all of its pixels, which a crop cuts from
#[cfg(feature = "image")]
#[derive(Clone)]
enum Original {
    // the image as it is sent, or the file of `image!`
    Encoded(Bytes),
    Converted(Arc<Vec<u8>>),
    Decoded(Arc<DynamicImage>),
}

#[cfg(feature = "image")]
impl Original {
    fn decode(&self) -> Result<(Cow<'_, DynamicImage>, Option<ImageFormat>), InvalidImage> {
        let bytes = match self {
            Original::Encoded(bytes) => bytes.as_slice(),
            Original::Converted(bytes) => bytes,
            Original::Decoded(image) => return Ok((Cow::Borrowed(image), None)),
        };
        let (image, format) = decode(bytes)?;
        Ok((Cow::Owned(image), format))
    }
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
    let Header { width, height } = static_header(bytes);
    Image {
        width,
        height,
        bytes: Bytes::Static(bytes),
        place: whole(width, height),
        #[cfg(feature = "image")]
        original: Original::Encoded(Bytes::Static(bytes)),
    }
}

// a file of `image!` that the feature converted, with the file as its original
#[cfg(feature = "image")]
#[doc(hidden)]
#[must_use]
pub const fn converted_image(
    converted: &'static [u8],
    file: &'static [u8],
    width: u32,
    height: u32,
) -> Image {
    let Header {
        width: converted_width,
        height: converted_height,
    } = static_header(converted);
    Image {
        width: converted_width,
        height: converted_height,
        bytes: Bytes::Static(converted),
        place: whole(width, height),
        original: Original::Encoded(Bytes::Static(file)),
    }
}

const fn static_header(bytes: &[u8]) -> Header {
    match read_header(bytes) {
        Ok(header) => header,
        Err(error) => panic!("{}", error.message()),
    }
}

/// Makes an [`Image`] that the compiler checks.
///
/// It takes a path, as `include_bytes!` does, or a constant of the type `&'static [u8]`. It
/// validates the image as [`Image::new`] does, when the crate builds. With the `image` feature, it
/// also converts the file of a path, and keeps the file as the original. A macro cannot read the
/// value of a constant, so it only validates a constant.
///
/// # Examples
///
/// ```
/// use system2::{Image, image};
///
/// const SQUARE: Image = image!("../tests/images/square.png");
/// const RED: Image = image!(include_bytes!("../tests/images/red.png"));
///
/// assert!(SQUARE.to_string().starts_with("s2-img-"));
/// ```
#[macro_export]
macro_rules! image {
    ($path:literal) => {
        $crate::__private::include_image!($path)
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
        let bytes = encode(&dynamic_image, None)?;
        let place = whole(dynamic_image.width(), dynamic_image.height());
        let original = Original::Decoded(Arc::new(dynamic_image));
        Image::with_original(bytes, place, original)
    }
}

#[cfg(feature = "image")]
impl TryFrom<&Image> for ::image::DynamicImage {
    type Error = InvalidImage;

    fn try_from(image: &Image) -> Result<Self, Self::Error> {
        let (original, _) = image.original.decode()?;
        let PixelBox {
            left,
            top,
            right,
            bottom,
        } = image.place;
        Ok(original.crop_imm(left, top, right - left, bottom - top))
    }
}

impl fmt::Display for Image {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reference())
    }
}

// the bytes can have megabytes, so it shows only their number, and not the original
#[cfg_attr(
    feature = "image",
    expect(
        clippy::missing_fields_in_debug,
        reason = "the original can have megabytes too"
    )
)]
impl fmt::Debug for Image {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.bytes.as_slice().len())
            .field("place", &self.place)
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
    #[cfg(feature = "image")]
    use {
        super::{Bytes, Original, converted_image, whole},
        ::image::{DynamicImage, ImageFormat},
        std::io::Cursor,
    };

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

        let not_json = "s2-img-3333333333333333 is not json";

        let found = references([state.as_str(), question, r#""no reference""#, not_json]);

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

    #[cfg(feature = "image")]
    #[test]
    fn converted_image_keeps_the_file_as_the_original() {
        let red = include_bytes!("../tests/images/red.png");

        let image = converted_image(red, b"the file", 200, 100);

        assert_eq!((image.width(), image.height()), (280, 280));
        assert_eq!(image.place(), whole(200, 100));
        assert!(matches!(
            image.original,
            Original::Encoded(Bytes::Static(b"the file"))
        ));
    }

    #[cfg(feature = "image")]
    #[test]
    fn crop_keeps_a_jpeg_and_makes_a_png_of_each_other_format() {
        let original = DynamicImage::new_rgb8(400, 300);
        let jpeg_start: &[u8] = &[0xFF, 0xD8];
        let png_start: &[u8] = b"\x89PNG\r\n\x1a\n";
        let cases = [
            (ImageFormat::Jpeg, jpeg_start),
            (ImageFormat::Png, png_start),
            (ImageFormat::WebP, png_start),
        ];
        for (format, start) in cases {
            let mut bytes = Cursor::new(Vec::new());
            original.write_to(&mut bytes, format).unwrap();
            let image = Image::new(bytes.into_inner()).unwrap();

            let crop = image.crop(whole(300, 300)).unwrap();

            assert!(crop.bytes.as_slice().starts_with(start), "{format:?}");
        }
    }
}
