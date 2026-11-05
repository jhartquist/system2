use system2::{Image, image};

// a png header of 400x300 px, then an exif chunk with the orientation 6
const BYTES: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\x01\x90\0\0\x01\x2c\x08\x02\0\0\0\0\0\0\0\
    \0\0\0\x16eXIfII\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";

const LOGO: Image = image!(BYTES);

fn main() {}
