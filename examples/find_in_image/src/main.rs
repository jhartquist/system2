//! Asks if a photo shows an object, and where, in one request. Then writes a crop of the object,
//! or the photo with a rectangle around the object, at the size of the original photo.

use clap::{Parser, ValueEnum};
use image::{DynamicImage, Rgb};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use system2::prelude::*;
use system2::{Bbox, Image, PixelBox};

/// The lowest probability that the photo shows the object.
const PRESENCE_THRESHOLD: f64 = 0.7;

const RECTANGLE_COLOR: Rgb<u8> = Rgb([255, 40, 40]);

#[derive(Parser)]
struct Args {
    /// A photo in PNG, JPEG, WebP, or GIF, of any size.
    input_path: PathBuf,
    /// What to find, such as "the coffee cup".
    target_object: String,
    /// A crop of the object, or the photo with a rectangle around the object.
    #[arg(value_enum)]
    output_type: OutputType,
    /// The file to write, such as cup.png.
    output_path: PathBuf,
}

#[derive(Clone, ValueEnum)]
enum OutputType {
    Crop,
    Draw,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let input_image = Image::new(fs::read(&args.input_path)?)?;
    let target_object = &args.target_object;

    let answers = questions! {
        is_present: noul!("Is {target_object} in the photo?"),
        bbox: Bbox::new(target_object)?,
    }
    .ask(&input_image)
    .await?;

    let probability = answers.is_present.probability();
    if probability < PRESENCE_THRESHOLD {
        println!("not found: {target_object} (probability {probability:.2})");
        return Ok(());
    }

    let pixel_box = answers.bbox.to_pixels(&input_image);
    let original_image = DynamicImage::try_from(&input_image)?;
    let output_image = match args.output_type {
        OutputType::Crop => crop(&original_image, pixel_box),
        OutputType::Draw => draw_rectangle(original_image, pixel_box),
    };
    output_image.save(&args.output_path)?;

    let PixelBox {
        left,
        top,
        right,
        bottom,
    } = pixel_box;
    let confidence = answers.bbox.confidence();
    println!(
        "found: {target_object} at ({left}, {top}) to ({right}, {bottom}) \
         (probability {probability:.2}, confidence {confidence:.2})"
    );

    Ok(())
}

fn crop(original_image: &DynamicImage, pixel_box: PixelBox) -> DynamicImage {
    let PixelBox {
        left,
        top,
        right,
        bottom,
    } = pixel_box;
    original_image.crop_imm(left, top, right - left, bottom - top)
}

/// Draws the rectangle inside the edges of the box, with lines that grow with the photo.
fn draw_rectangle(original_image: DynamicImage, pixel_box: PixelBox) -> DynamicImage {
    let mut rgb_image = original_image.into_rgb8();
    let thickness = (rgb_image.width().max(rgb_image.height()) / 400).max(1);
    let PixelBox {
        left,
        top,
        right,
        bottom,
    } = pixel_box;
    for y in top..bottom {
        for x in left..right {
            let on_edge = x < left + thickness
                || x + thickness >= right
                || y < top + thickness
                || y + thickness >= bottom;
            if on_edge {
                rgb_image.put_pixel(x, y, RECTANGLE_COLOR);
            }
        }
    }
    rgb_image.into()
}
