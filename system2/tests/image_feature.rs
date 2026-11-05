#![cfg(feature = "image")]

mod support;

use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use support::{encoded, png};
use system2::{Image, image};

const SMALL: Image = image!("images/small.webp");
const SMALL_BYTES: &[u8] = include_bytes!("images/small.webp");

#[test]
fn new_converts_each_format_size_and_orientation() {
    let gif = encoded(&DynamicImage::new_rgb8(140, 100), ImageFormat::Gif);
    let large = encoded(&DynamicImage::new_rgb8(2000, 1000), ImageFormat::Png);

    // 400x300 px, with the exif orientation 6
    let sideways = include_bytes!("images/sideways.jpg").to_vec();
    let cases = [
        (SMALL_BYTES.to_vec(), (560, 280)),
        (gif, (392, 280)),
        (sideways, (300, 400)),
        (large, (1568, 784)),
    ];
    for (bytes, size) in cases {
        let image = Image::new(bytes).unwrap();

        assert_eq!((image.width(), image.height()), size);
    }
}

#[test]
fn new_refuses_an_image_that_it_cannot_convert() {
    let cases = [
        (
            b"GIF89a".to_vec(),
            "the image must be a valid PNG, JPEG, WebP, or GIF",
        ),
        (
            include_bytes!("images/long.png").to_vec(),
            "the long side of the image must be at most 5.6 times its short side, \
             not 1700x300: crop it",
        ),
    ];
    for (bytes, message) in cases {
        let error = Image::new(bytes).unwrap_err();

        assert_eq!(error.to_string(), message);
    }
}

#[test]
fn image_converts_a_file_when_the_crate_builds() {
    let from_data = Image::new(SMALL_BYTES).unwrap();

    assert_eq!((SMALL.width(), SMALL.height()), (560, 280));
    assert_eq!(SMALL.to_string(), from_data.to_string());
}

#[test]
fn try_from_converts_to_and_from_a_dynamic_image() {
    let checks = RgbImage::from_fn(300, 300, |x, y| Rgb([255 * u8::from((x + y) % 2 == 0); 3]));
    let checks = DynamicImage::from(checks);

    let drawn = Image::try_from(checks.clone()).unwrap();
    let scaled = Image::try_from(DynamicImage::new_rgb8(2000, 1000)).unwrap();
    let pixels = DynamicImage::try_from(&drawn).unwrap();

    // a png keeps each pixel, and a jpeg would not
    assert_eq!(pixels, checks);
    assert_eq!((scaled.width(), scaled.height()), (1568, 784));
}

#[test]
fn try_from_refuses_an_image_that_does_not_fit_or_decode() {
    let too_long = Image::try_from(DynamicImage::new_rgb8(1700, 300)).unwrap_err();

    // `Image::new` reads only the header, so the data can still be broken
    let header_only = Image::new(png(400, 300)).unwrap();
    let broken = DynamicImage::try_from(&header_only).unwrap_err();

    assert_eq!(
        too_long.to_string(),
        "the long side of the image must be at most 5.6 times its short side, \
         not 1700x300: crop it"
    );
    assert_eq!(
        broken.to_string(),
        "the image must be a valid PNG, JPEG, WebP, or GIF"
    );
}
