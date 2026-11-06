#![cfg(feature = "image")]

mod support;

use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use support::{bbox_answer, client_for, encoded, mock_api, png, response};
use system2::prelude::*;
use system2::{AskError, Bbox, BboxAnswer, Image, InvalidImage, PixelBox};

const SMALL: Image = image!("images/small.webp");
const SMALL_BYTES: &[u8] = include_bytes!("images/small.webp");
const MUG: Bbox = bbox!("The mug.");

// a different color at each pixel, so that two different crops are never equal
fn photo(width: u32, height: u32) -> DynamicImage {
    let photo = RgbImage::from_fn(width, height, |x, y| {
        let [x_low, x_high, ..] = x.to_le_bytes();
        let [y_low, y_high, ..] = y.to_le_bytes();
        Rgb([x_low, y_low, x_high * 16 + y_high])
    });
    DynamicImage::from(photo)
}

fn pixel_box(left: u32, top: u32, right: u32, bottom: u32) -> PixelBox {
    PixelBox {
        left,
        top,
        right,
        bottom,
    }
}

async fn find_mug(
    image: &Image,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
) -> Result<BboxAnswer, AskError> {
    let server = mock_api(response(&bbox_answer(left, top, right, bottom, 0.9))).await;
    client_for(&server).ask(MUG, image).await
}

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

#[tokio::test]
async fn to_pixels_gives_the_pixels_of_the_original() {
    let large = encoded(&photo(2000, 1000), ImageFormat::Png);
    let passes = encoded(&photo(400, 300), ImageFormat::Png);
    let cases = [
        (Image::new(passes).unwrap(), pixel_box(100, 30, 300, 150)),
        (
            Image::new(large.clone()).unwrap(),
            pixel_box(500, 100, 1500, 500),
        ),
        (
            Image::new(include_bytes!("images/sideways.jpg")).unwrap(),
            pixel_box(75, 40, 225, 200),
        ),
        (SMALL, pixel_box(50, 10, 150, 50)),
        (
            Image::try_from(photo(2000, 1000)).unwrap(),
            pixel_box(500, 100, 1500, 500),
        ),
        (
            Image::new(large)
                .unwrap()
                .crop(pixel_box(1000, 200, 1400, 600))
                .unwrap(),
            pixel_box(1100, 240, 1300, 400),
        ),
    ];
    for (image, pixels) in cases {
        let found = find_mug(&image, 0.25, 0.1, 0.75, 0.5).await.unwrap();

        assert_eq!(found.to_pixels(&image), pixels, "{image:?}");
    }
}

#[test]
fn crop_cuts_the_region_from_each_original_with_all_of_its_detail() {
    let large = photo(2000, 1000);
    let passes = photo(600, 400);
    let converted = Image::new(encoded(&large, ImageFormat::Png)).unwrap();
    let crop_of_converted = converted.crop(pixel_box(0, 0, 1600, 800)).unwrap();
    let cases = [
        (&converted, &large, pixel_box(1000, 200, 1300, 500)),
        (&crop_of_converted, &large, pixel_box(1500, 600, 1800, 900)),
        (
            &Image::try_from(large.clone()).unwrap(),
            &large,
            pixel_box(1000, 200, 1300, 500),
        ),
        (
            &Image::new(encoded(&passes, ImageFormat::Png)).unwrap(),
            &passes,
            pixel_box(300, 100, 600, 400),
        ),
    ];
    for (image, original, region) in cases {
        let PixelBox {
            left,
            top,
            right,
            bottom,
        } = region;

        let crop = image.crop(region).unwrap();

        let cut = original.crop_imm(left, top, right - left, bottom - top);
        assert_eq!(DynamicImage::try_from(&crop).unwrap(), cut, "{region:?}");
    }
}

// the file has 200x100 px, and `image!` scales it to 560x280 px
#[test]
fn crop_cuts_a_file_of_image_from_the_file() {
    let crop = SMALL.crop(pixel_box(0, 0, 200, 100)).unwrap();
    let error = SMALL.crop(pixel_box(0, 0, 560, 280)).unwrap_err();

    assert_eq!((crop.width(), crop.height()), (560, 280));
    assert!(matches!(
        error,
        InvalidImage::Region {
            width: 200,
            height: 100,
            ..
        }
    ));
}

#[test]
fn crop_refuses_a_region_that_does_not_fit() {
    let image = Image::new(encoded(&photo(600, 400), ImageFormat::Png)).unwrap();
    let regions = [
        pixel_box(300, 0, 601, 300),
        pixel_box(0, 100, 300, 401),
        pixel_box(100, 100, 100, 300),
        pixel_box(300, 0, 0, 300),
        pixel_box(0, 0, 600, 100),
        pixel_box(0, 0, 70, 400),
    ];
    for region in regions {
        let error = image.crop(region).unwrap_err();

        assert!(
            matches!(
                error,
                InvalidImage::Region { region: refused, width: 600, height: 400 }
                    if refused == region
            ),
            "{region:?}"
        );
    }
    assert_eq!(
        image.crop(regions[0]).unwrap_err().to_string(),
        "the region must be in the original of 600x400 px, have an area, and have a long side \
         at most 5.6 times its short side, not (300, 0) to (601, 300)"
    );
}

#[test]
fn crop_refuses_an_original_that_does_not_decode() {
    let header_only = Image::new(png(400, 300)).unwrap();

    let error = header_only.crop(pixel_box(0, 0, 300, 300)).unwrap_err();

    assert!(matches!(error, InvalidImage::Decode(_)));
}
