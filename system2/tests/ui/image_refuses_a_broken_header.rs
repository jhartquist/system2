use system2::{Image, image};

// a png signature with no header chunk
const BYTES: &[u8] = b"\x89PNG\r\n\x1a\n";

const LOGO: Image = image!(BYTES);

fn main() {}
