// the readers are `const fn`, so `image!` validates a file by the rules of `Image::new`, and a
// `const fn` cannot take a range of a slice, so they take offsets

pub(crate) const MIN_SIDE: u32 = 280;
pub(crate) const MAX_SIDE: u32 = 1568;
const MAX_BYTES: usize = 16 * 1024 * 1024;

pub const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

// the signature, then the length, the type, the 13 bytes of data, and the crc of the header chunk
const PNG_HEADER_LENGTH: u32 = 13;
const PNG_HEADER_END: usize = 33;
const PNG_CHUNK_START: usize = 8;
const PNG_CRC: usize = 4;
const JPEG_START: &[u8] = &[0xFF, 0xD8];
const EXIF_START: &[u8] = b"Exif\0\0";
const ORIENTATION_TAG: u16 = 0x0112;
const UPRIGHT: u16 = 1;
const TIFF_ENTRY: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub width: u32,
    pub height: u32,
}

// `Copy`, so a constant can drop it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderError {
    Format,
    Header,
    Size { width: u32, height: u32 },
    Bytes { size: usize },
    Orientation { orientation: u16 },
}

// with the feature, `image!` converts a file, but it cannot read the value of a constant
#[cfg(not(feature = "image"))]
macro_rules! fix {
    ($action:literal) => {
        concat!("turn on the `image` feature of system2 to ", $action)
    };
}

#[cfg(feature = "image")]
macro_rules! fix {
    ($action:literal) => {
        concat!(
            "pass a path to `image!`, or call `Image::new`, to ",
            $action
        )
    };
}

impl HeaderError {
    // a panic in a constant takes no numbers, so the message of `image!` has none
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            HeaderError::Format => concat!("the image must be PNG or JPEG: ", fix!("convert it")),
            HeaderError::Header => "the image must have a complete PNG or JPEG header",
            HeaderError::Size { .. } => concat!(
                "each side of the image must be from 280 to 1568 px: ",
                fix!("scale it")
            ),
            HeaderError::Bytes { .. } => concat!(
                "the image must have at most 16 MiB: ",
                fix!("make it smaller")
            ),
            HeaderError::Orientation { .. } => concat!(
                "the image must be upright, but its EXIF orientation turns or mirrors it, and the \
                 API ignores the orientation: ",
                fix!("turn its pixels")
            ),
        }
    }
}

pub const fn read_header(bytes: &[u8]) -> Result<Header, HeaderError> {
    if bytes.len() > MAX_BYTES {
        let size = bytes.len();
        return Err(HeaderError::Bytes { size });
    }
    let read = if same_at(bytes, 0, PNG_SIGNATURE) {
        read_png(bytes)
    } else if same_at(bytes, 0, JPEG_START) {
        read_jpeg(bytes)
    } else {
        return Err(HeaderError::Format);
    };
    let (header, orientation) = match read {
        Ok(read) => read,
        Err(error) => return Err(error),
    };
    let Header { width, height } = header;
    if !fits(width) || !fits(height) {
        return Err(HeaderError::Size { width, height });
    }

    // 2 to 8 turn or mirror the pixels, and other values mean nothing
    match orientation {
        2..=8 => Err(HeaderError::Orientation { orientation }),
        _ => Ok(header),
    }
}

pub(crate) const fn fits(side: u32) -> bool {
    side >= MIN_SIDE && side <= MAX_SIDE
}

// the header and the exif orientation, upright if the image has none
type Read = Result<(Header, u16), HeaderError>;

// the header chunk comes first, and an exif chunk counts only before the image data
const fn read_png(bytes: &[u8]) -> Read {
    if bytes.len() < PNG_HEADER_END
        || u32_at(bytes, 8, true) != PNG_HEADER_LENGTH
        || !same_at(bytes, 12, b"IHDR")
    {
        return Err(HeaderError::Header);
    }
    let width = u32_at(bytes, 16, true);
    let height = u32_at(bytes, 20, true);
    let mut orientation = UPRIGHT;
    let mut chunk = PNG_SIGNATURE.len();
    while chunk + PNG_CHUNK_START <= bytes.len() {
        let data = chunk + PNG_CHUNK_START;
        let end = data.saturating_add(u32_at(bytes, chunk, true) as usize);
        if same_at(bytes, chunk + 4, b"IDAT") || end > bytes.len() {
            break;
        }
        if same_at(bytes, chunk + 4, b"eXIf") {
            orientation = exif_orientation(bytes, data, end);
        }
        chunk = end + PNG_CRC;
    }
    Ok((Header { width, height }, orientation))
}

// walks the segments to the first frame header, and reads exif from an app1 segment on the way
const fn read_jpeg(bytes: &[u8]) -> Read {
    let mut orientation = UPRIGHT;
    let mut segment = JPEG_START.len();
    while segment + 4 <= bytes.len() {
        if bytes[segment] != 0xFF {
            return Err(HeaderError::Header);
        }
        let marker = bytes[segment + 1];
        let data = segment + 4;
        let end = segment + 2 + u16_at(bytes, segment + 2, true) as usize;
        if end > bytes.len() {
            return Err(HeaderError::Header);
        }
        if is_frame(marker) {
            if end < data + 5 {
                return Err(HeaderError::Header);
            }
            let height = u16_at(bytes, data + 1, true) as u32;
            let width = u16_at(bytes, data + 3, true) as u32;
            return Ok((Header { width, height }, orientation));
        }
        if marker == 0xE1 && same_at(bytes, data, EXIF_START) {
            orientation = exif_orientation(bytes, data + EXIF_START.len(), end);
        }
        segment = end;
    }
    Err(HeaderError::Header)
}

// sof0 to sof15, except dht, jpg, and dac, which share the range
const fn is_frame(marker: u8) -> bool {
    matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC)
}

// the orientation in the tiff data from `start` to `end`. the api ignores exif, so exif that
// cannot be read counts as upright, and does not refuse a photo that the api takes
const fn exif_orientation(bytes: &[u8], start: usize, end: usize) -> u16 {
    if start + 8 > end {
        return UPRIGHT;
    }
    let big_endian = if same_at(bytes, start, b"MM") {
        true
    } else if same_at(bytes, start, b"II") {
        false
    } else {
        return UPRIGHT;
    };
    let directory = start.saturating_add(u32_at(bytes, start + 4, big_endian) as usize);
    if directory.saturating_add(2) > end {
        return UPRIGHT;
    }
    let entry_count = u16_at(bytes, directory, big_endian) as usize;
    let mut entry_index = 0;
    while entry_index < entry_count {
        let entry = directory + 2 + entry_index * TIFF_ENTRY;
        if entry + TIFF_ENTRY > end {
            return UPRIGHT;
        }
        if u16_at(bytes, entry, big_endian) == ORIENTATION_TAG {
            return u16_at(bytes, entry + 8, big_endian);
        }
        entry_index += 1;
    }
    UPRIGHT
}

// `==` on slices is not const
const fn same_at(bytes: &[u8], at: usize, expected: &[u8]) -> bool {
    if at + expected.len() > bytes.len() {
        return false;
    }
    let mut index = 0;
    while index < expected.len() {
        if bytes[at + index] != expected[index] {
            return false;
        }
        index += 1;
    }
    true
}

const fn u16_at(bytes: &[u8], at: usize, big_endian: bool) -> u16 {
    let pair = [bytes[at], bytes[at + 1]];
    if big_endian {
        u16::from_be_bytes(pair)
    } else {
        u16::from_le_bytes(pair)
    }
}

const fn u32_at(bytes: &[u8], at: usize, big_endian: bool) -> u32 {
    let quad = [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
    if big_endian {
        u32::from_be_bytes(quad)
    } else {
        u32::from_le_bytes(quad)
    }
}

#[cfg(test)]
mod tests {
    use super::{Header, HeaderError, read_header};

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut header = [width.to_be_bytes(), height.to_be_bytes()].concat();
        header.extend([8, 2, 0, 0, 0]);
        [&b"\x89PNG\r\n\x1a\n"[..], &png_chunk(b"IHDR", &header)].concat()
    }

    fn png_chunk(chunk_type: &[u8], data: &[u8]) -> Vec<u8> {
        let length = u32::try_from(data.len()).unwrap().to_be_bytes();
        [&length[..], chunk_type, data, &[0; 4]].concat()
    }

    fn jpeg_segment(marker: u8, data: &[u8]) -> Vec<u8> {
        let length = u16::try_from(data.len() + 2).unwrap().to_be_bytes();
        [&[0xFF, marker][..], &length, data].concat()
    }

    // a jpeg with this frame marker, and an app1 segment before the frame if one is given
    fn jpeg_with(width: u16, height: u16, marker: u8, app1: Option<&[u8]>) -> Vec<u8> {
        let mut frame = vec![8];
        frame.extend(height.to_be_bytes());
        frame.extend(width.to_be_bytes());
        frame.extend([1, 1, 0x11, 0]);
        let app1 = app1
            .map(|data| jpeg_segment(0xE1, data))
            .unwrap_or_default();
        [
            &[0xFF, 0xD8][..],
            &app1,
            &jpeg_segment(marker, &frame),
            &[0xFF, 0xD9],
        ]
        .concat()
    }

    fn jpeg(width: u16, height: u16) -> Vec<u8> {
        jpeg_with(width, height, 0xC0, None)
    }

    // tiff data with one directory entry, the orientation
    fn tiff(orientation: u16, big_endian: bool) -> Vec<u8> {
        let u16_bytes = |value: u16| {
            if big_endian {
                value.to_be_bytes()
            } else {
                value.to_le_bytes()
            }
        };
        let u32_bytes = |value: u32| {
            if big_endian {
                value.to_be_bytes()
            } else {
                value.to_le_bytes()
            }
        };
        let order: &[u8] = if big_endian { b"MM" } else { b"II" };
        [
            order,
            &u16_bytes(42),
            &u32_bytes(8),
            &u16_bytes(1),
            &u16_bytes(0x0112),
            &u16_bytes(3),
            &u32_bytes(1),
            &u16_bytes(orientation),
            &[0, 0],
        ]
        .concat()
    }

    fn exif(tiff: &[u8]) -> Vec<u8> {
        [&b"Exif\0\0"[..], tiff].concat()
    }

    fn header(width: u32, height: u32) -> Header {
        Header { width, height }
    }

    #[test]
    fn read_header_reads_the_size_of_a_png_and_a_jpeg() {
        let progressive = jpeg_with(500, 700, 0xC2, None);
        let after_xmp = jpeg_with(
            640,
            480,
            0xC0,
            Some(b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta/>"),
        );

        // a huffman table shares the range of the frame markers
        let after_table = [
            &[0xFF, 0xD8][..],
            &jpeg_segment(0xC4, &[0; 20]),
            &jpeg(640, 480)[2..],
        ]
        .concat();
        let cases = [
            (png(400, 300), Ok(header(400, 300))),
            (jpeg(640, 480), Ok(header(640, 480))),
            (progressive, Ok(header(500, 700))),
            (after_xmp, Ok(header(640, 480))),
            (after_table, Ok(header(640, 480))),
        ];
        for (bytes, header) in cases {
            assert_eq!(read_header(&bytes), header, "{bytes:?}");
        }
    }

    #[test]
    fn read_header_takes_each_side_from_280_to_1568_px() {
        let cases = [
            (280, 280, true),
            (1568, 1568, true),
            (279, 500, false),
            (500, 279, false),
            (1569, 500, false),
            (500, 1569, false),
        ];
        for (width, height, valid) in cases {
            let expected = if valid {
                Ok(header(width.into(), height.into()))
            } else {
                Err(HeaderError::Size {
                    width: width.into(),
                    height: height.into(),
                })
            };

            assert_eq!(read_header(&png(width.into(), height.into())), expected);
            assert_eq!(read_header(&jpeg(width, height)), expected);
        }
    }

    #[test]
    fn read_header_takes_at_most_16_mib() {
        let mut bytes = png(300, 300);
        bytes.resize(16 * 1024 * 1024, 0);
        assert_eq!(read_header(&bytes), Ok(header(300, 300)));

        bytes.push(0);
        assert_eq!(
            read_header(&bytes),
            Err(HeaderError::Bytes { size: 16_777_217 })
        );
    }

    #[test]
    fn read_header_refuses_other_formats() {
        let others: [&[u8]; 4] = [
            b"",
            b"GIF89a\x01\x00\x01\x00",
            b"RIFF\0\0\0\0WEBPVP8 ",
            b"\xFF",
        ];
        for bytes in others {
            assert_eq!(read_header(bytes), Err(HeaderError::Format), "{bytes:?}");
        }
    }

    #[test]
    fn read_header_refuses_a_broken_header() {
        let signature = b"\x89PNG\r\n\x1a\n".to_vec();
        let short_png = png(300, 300)[..20].to_vec();
        let no_crc = png(300, 300)[..32].to_vec();
        let mut long_header = png(300, 300);
        long_header[11] = 14;
        let data_first = [&signature[..], &png_chunk(b"IDAT", &[0; 13])].concat();
        let no_frame = vec![0xFF, 0xD8, 0xFF, 0xD9];
        let past_the_end = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x10, 0x00, 0x00];
        let short_frame = [
            &[0xFF, 0xD8][..],
            &jpeg_segment(0xC0, &[8, 1, 0, 2]),
            &[0xFF, 0xD9],
        ]
        .concat();
        let not_a_marker = vec![0xFF, 0xD8, 0x00, 0xC0, 0x00, 0x11];
        let cases = [
            signature,
            short_png,
            no_crc,
            long_header,
            data_first,
            no_frame,
            past_the_end,
            short_frame,
            not_a_marker,
        ];
        for bytes in cases {
            assert_eq!(read_header(&bytes), Err(HeaderError::Header), "{bytes:?}");
        }
    }

    #[test]
    fn read_header_refuses_an_image_that_its_exif_turns() {
        let png_exif = [png(400, 300), png_chunk(b"eXIf", &tiff(3, true))].concat();
        let cases = [
            (jpeg_with(640, 480, 0xC0, Some(&exif(&tiff(6, false)))), 6),
            (jpeg_with(640, 480, 0xC0, Some(&exif(&tiff(8, true)))), 8),
            (jpeg_with(640, 480, 0xC0, Some(&exif(&tiff(2, false)))), 2),
            (png_exif, 3),
        ];
        for (bytes, orientation) in cases {
            assert_eq!(
                read_header(&bytes),
                Err(HeaderError::Orientation { orientation })
            );
        }
    }

    #[test]
    fn read_header_takes_an_upright_image() {
        let upright = jpeg_with(640, 480, 0xC0, Some(&exif(&tiff(1, false))));
        let unknown = jpeg_with(640, 480, 0xC0, Some(&exif(&tiff(9, false))));
        let png_upright = [png(400, 300), png_chunk(b"eXIf", &tiff(1, true))].concat();

        // png puts exif before the image data, so exif after it does not count
        let after_data = [
            png(400, 300),
            png_chunk(b"IDAT", &[0; 4]),
            png_chunk(b"eXIf", &tiff(6, true)),
        ]
        .concat();
        let chunk_past_the_end = [&png(400, 300)[..], &100u32.to_be_bytes(), b"eXIf"].concat();
        for bytes in [
            upright,
            unknown,
            png_upright,
            after_data,
            chunk_past_the_end,
        ] {
            assert!(read_header(&bytes).is_ok(), "{bytes:?}");
        }
    }

    #[test]
    fn read_header_ignores_exif_that_it_cannot_read() {
        let mut other_tag = tiff(6, false);
        other_tag[10..12].copy_from_slice(&0x0110u16.to_le_bytes());
        let other_order = [&b"XX"[..], &tiff(6, false)[2..]].concat();
        let short_entry = tiff(6, false)[..21].to_vec();
        let far_directory = [&b"II\x2a\x00"[..], &500u32.to_le_bytes()].concat();
        let cases = [
            other_tag,
            other_order,
            short_entry,
            far_directory,
            Vec::new(),
        ];
        for tiff in cases {
            let bytes = jpeg_with(640, 480, 0xC0, Some(&exif(&tiff)));

            assert_eq!(read_header(&bytes), Ok(header(640, 480)), "{tiff:?}");
        }
    }

    #[test]
    fn message_tells_the_rule() {
        let errors = [
            HeaderError::Format,
            HeaderError::Header,
            HeaderError::Size {
                width: 1,
                height: 1,
            },
            HeaderError::Bytes { size: 1 },
            HeaderError::Orientation { orientation: 6 },
        ];
        #[cfg(not(feature = "image"))]
        let fix = "turn on the `image` feature of system2 to";
        #[cfg(feature = "image")]
        let fix = "pass a path to `image!`, or call `Image::new`, to";
        let messages = [
            format!("the image must be PNG or JPEG: {fix} convert it"),
            "the image must have a complete PNG or JPEG header".to_owned(),
            format!("each side of the image must be from 280 to 1568 px: {fix} scale it"),
            format!("the image must have at most 16 MiB: {fix} make it smaller"),
            format!(
                "the image must be upright, but its EXIF orientation turns or mirrors it, and the \
                 API ignores the orientation: {fix} turn its pixels"
            ),
        ];
        assert_eq!(errors.map(HeaderError::message), messages);
    }
}
