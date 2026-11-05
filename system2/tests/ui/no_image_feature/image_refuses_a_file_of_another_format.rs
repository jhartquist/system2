use system2::{Image, image};

// a rust file is not an image
const LOGO: Image = image!("image_refuses_a_file_of_another_format.rs");

fn main() {}
