use super::header::{Header, MAX_SIDE, MIN_SIDE, fits, read_header};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageError, ImageFormat, ImageReader, Limits};
use std::fmt;
use std::io::Cursor;

// a photo of 12 megapixels becomes about 60 KB at 1568x1176
const JPEG_QUALITY: u8 = 90;

#[derive(Debug)]
pub enum ConvertError {
    Decode(ImageError),
    Size { width: u32, height: u32 },
}

// the image that the model sees, and the size of the upright original
#[derive(Debug)]
pub struct Converted {
    pub bytes: Vec<u8>,
    pub original: Header,
}

// none when the image passes as it is, so that it keeps its bytes
pub fn convert(bytes: &[u8]) -> Result<Option<Converted>, ConvertError> {
    if read_header(bytes).is_ok() {
        return Ok(None);
    }
    let (image, format) = decode(bytes)?;
    let original = Header {
        width: image.width(),
        height: image.height(),
    };
    let bytes = encode(&image, format)?;
    Ok(Some(Converted { bytes, original }))
}

// the pixels upright by the exif orientation, because the api ignores it
pub fn decode(bytes: &[u8]) -> Result<(DynamicImage, Option<ImageFormat>), ImageError> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(ImageError::IoError)?;
    let format = reader.format();
    let mut decoder = reader.into_decoder()?;

    // the header of a small file can give a very large size, and `from_decoder` allocates
    // the full image with no memory limit. so check the limit before the decode
    Limits::default().reserve(decoder.total_bytes())?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok((image, format))
}

// a jpeg stays a jpeg, and each other format becomes a png, so that text stays sharp. each
// channel gets 8 bits, so that a png of 1568x1568 px stays under 16 MiB
pub fn encode(image: &DynamicImage, format: Option<ImageFormat>) -> Result<Vec<u8>, ConvertError> {
    let (width, height) = (image.width(), image.height());
    let Some((fitted_width, fitted_height)) = fitted_size(width, height) else {
        return Err(ConvertError::Size { width, height });
    };
    let image = image.resize_exact(fitted_width, fitted_height, FilterType::Lanczos3);
    let mut bytes = Vec::new();
    if format == Some(ImageFormat::Jpeg) {
        let encoder = JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY);
        image.to_rgb8().write_with_encoder(encoder)?;
    } else if image.color().has_alpha() {
        image
            .to_rgba8()
            .write_with_encoder(PngEncoder::new(&mut bytes))?;
    } else {
        image
            .to_rgb8()
            .write_with_encoder(PngEncoder::new(&mut bytes))?;
    }
    Ok(bytes)
}

// one factor for both sides keeps the aspect ratio. none when no factor fits both sides
#[must_use]
pub fn fitted_size(width: u32, height: u32) -> Option<(u32, u32)> {
    let (long, short) = (width.max(height), width.min(height));
    let (fitted_long, fitted_short) = if long > MAX_SIDE {
        (MAX_SIDE, scaled(short, MAX_SIDE, long)?)
    } else if short < MIN_SIDE {
        (scaled(long, MIN_SIDE, short)?, MIN_SIDE)
    } else {
        (long, short)
    };
    if !fits(fitted_long) || !fits(fitted_short) {
        return None;
    }
    if width >= height {
        Some((fitted_long, fitted_short))
    } else {
        Some((fitted_short, fitted_long))
    }
}

// side * to / from, rounded, with no floats to cast
fn scaled(side: u32, to: u32, from: u32) -> Option<u32> {
    let product = u64::from(side) * u64::from(to) + u64::from(from / 2);
    u32::try_from(product.checked_div(u64::from(from))?).ok()
}

impl From<ImageError> for ConvertError {
    fn from(error: ImageError) -> Self {
        ConvertError::Decode(error)
    }
}

impl fmt::Display for ConvertError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::Decode(error) => write!(
                formatter,
                "the image must be a valid PNG, JPEG, WebP, or GIF: {error}"
            ),
            ConvertError::Size { width, height } => write!(
                formatter,
                "the long side of the image must be at most 5.6 times its short side, \
                 not {width}x{height}: crop it"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Header, read_header};
    use super::{ConvertError, Converted, convert, encode, fitted_size};
    use image::{DynamicImage, ImageError, ImageFormat};
    use std::io::Cursor;

    fn encoded(image: &DynamicImage, format: ImageFormat) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn convert_keeps_an_image_that_passes() {
        let png = encoded(&DynamicImage::new_rgb8(300, 300), ImageFormat::Png);

        assert!(convert(&png).unwrap().is_none());
    }

    #[test]
    fn convert_keeps_a_jpeg_and_makes_a_png_of_each_other_format() {
        let small = DynamicImage::new_rgb8(140, 100);
        let jpeg_start: &[u8] = &[0xFF, 0xD8];
        let png_start: &[u8] = b"\x89PNG\r\n\x1a\n";
        let cases = [
            (ImageFormat::Jpeg, jpeg_start),
            (ImageFormat::Png, png_start),
            (ImageFormat::WebP, png_start),
            (ImageFormat::Gif, png_start),
        ];
        for (format, start) in cases {
            let Converted { bytes, original } = convert(&encoded(&small, format)).unwrap().unwrap();

            assert!(bytes.starts_with(start), "{format:?}");
            assert_eq!(
                read_header(&bytes),
                Ok(Header {
                    width: 392,
                    height: 280
                })
            );
            assert_eq!(
                original,
                Header {
                    width: 140,
                    height: 100
                }
            );
        }
    }

    #[test]
    fn convert_refuses_an_image_that_does_not_decode() {
        let cases: [&[u8]; 3] = [b"GIF89a", b"not an image", b"RIFF\0\0\0\0WEBPVP8 "];
        for bytes in cases {
            assert!(
                matches!(convert(bytes), Err(ConvertError::Decode(_))),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn convert_refuses_an_image_that_needs_too_much_memory() {
        // a gif of 16000x16000 px, which is 1 GiB of pixels: the header, the size of the screen,
        // and the start of one frame of that size
        let side = 16000_u16.to_le_bytes();
        let screen = [&side[..], &side, &[0, 0, 0]].concat();
        let frame = [&b",\0\0\0\0"[..], &side, &side, &[0, 2, 0, b';']].concat();
        let gif = [&b"GIF89a"[..], &screen, &frame].concat();

        let error = convert(&gif).unwrap_err();

        assert!(
            matches!(error, ConvertError::Decode(ImageError::Limits(_))),
            "{error:?}"
        );
    }

    #[test]
    fn encode_writes_8_bit_and_keeps_alpha() {
        let cases = [
            (DynamicImage::new_rgba16(300, 300), 6),
            (DynamicImage::new_rgb16(300, 300), 2),
            (DynamicImage::new_luma_a8(300, 300), 6),
        ];
        for (image, color_type) in cases {
            let png = encode(&image, None).unwrap();

            // the bit depth and the color type in the header chunk
            assert_eq!(png[24..26], [8, color_type], "{:?}", image.color());
        }
    }

    #[test]
    fn encode_refuses_an_image_too_long_to_fit() {
        let error = encode(&DynamicImage::new_rgb8(1700, 300), None).unwrap_err();

        assert!(matches!(
            error,
            ConvertError::Size {
                width: 1700,
                height: 300
            }
        ));
    }

    #[test]
    fn fitted_size_scales_both_sides_by_one_factor() {
        let cases = [
            ((300, 300), Some((300, 300))),
            ((280, 1568), Some((280, 1568))),
            ((2000, 1000), Some((1568, 784))),
            ((1000, 2000), Some((784, 1568))),
            ((100, 50), Some((560, 280))),
            ((3136, 561), Some((1568, 281))),
            ((1700, 300), None),
            ((10, 100), None),
            ((0, 0), None),
            ((u32::MAX, 1), None),
        ];
        for ((width, height), fitted) in cases {
            assert_eq!(fitted_size(width, height), fitted, "{width}x{height}");
        }
    }

    #[test]
    fn fmt_tells_the_rule_and_the_fix() {
        let decode = convert(b"not an image").unwrap_err();
        let size = ConvertError::Size {
            width: 1700,
            height: 300,
        };

        assert_eq!(
            decode.to_string(),
            "the image must be a valid PNG, JPEG, WebP, or GIF: \
             The image format could not be determined"
        );
        assert_eq!(
            size.to_string(),
            "the long side of the image must be at most 5.6 times its short side, \
             not 1700x300: crop it"
        );
    }
}
