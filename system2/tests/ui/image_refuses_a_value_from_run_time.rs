use system2::image;

fn main() {
    let bytes = std::fs::read("logo.png").unwrap();
    let _ = image!(&bytes);
}
