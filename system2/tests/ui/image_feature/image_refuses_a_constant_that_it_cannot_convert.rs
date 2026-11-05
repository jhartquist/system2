use system2::{Image, image};

// a png header of 279x280 px
const BYTES: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\x01\x17\0\0\x01\x18\x08\x02\0\0\0\0\0\0\0";

const LOGO: Image = image!(BYTES);

fn main() {}
