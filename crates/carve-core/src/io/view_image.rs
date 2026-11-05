//! Saving a picture of a view: the front end renders the view to a PNG and
//! sends it as base64; this writes it as PNG, JPEG, BMP or GIF, chosen by
//! the file's extension.

use std::io::Cursor;
use std::path::Path as FsPath;

use image::ImageFormat;

use crate::error::CoreError;

/// The format a file name asks for: PNG, JPEG, BMP or GIF, PNG when it
/// names none of them.
pub fn format_of_path(path: &FsPath) -> ImageFormat {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        "bmp" => ImageFormat::Bmp,
        "gif" => ImageFormat::Gif,
        _ => ImageFormat::Png,
    }
}

/// Value of one base64 character, `None` for anything else.
fn sextet(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

/// Decodes standard or URL safe base64, ignoring white space and padding.
/// A `data:...;base64,` prefix is skipped.
///
/// # Errors
/// Returns [`CoreError::Validation`] for a character outside the alphabet
/// or a length no encoder produces.
pub fn decode_base64(text: &str) -> Result<Vec<u8>, CoreError> {
    let body = match text.find(";base64,") {
        Some(at) if text.starts_with("data:") => &text[at + 8..],
        _ => text,
    };
    let mut out = Vec::with_capacity(body.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut held = 0;
    for c in body.bytes() {
        if c.is_ascii_whitespace() || c == b'=' {
            continue;
        }
        let v = sextet(c).ok_or_else(|| CoreError::validation("the image data is not base64"))?;
        acc = (acc << 6) | v;
        held += 6;
        if held >= 8 {
            held -= 8;
            out.push(((acc >> held) & 0xFF) as u8);
        }
    }
    if held >= 6 {
        return Err(CoreError::validation("the image data is cut short"));
    }
    Ok(out)
}

/// Re-encodes a PNG as `format`. The other formats are written without
/// alpha, so the image is laid on white first; GIF reduces the colours to a
/// palette of 256.
///
/// # Errors
/// Returns [`CoreError::Format`] when `png` does not decode or the image
/// cannot be encoded.
pub fn convert(png: &[u8], format: ImageFormat) -> Result<Vec<u8>, CoreError> {
    let image = image::load_from_memory_with_format(png, ImageFormat::Png)
        .map_err(|error| CoreError::format("PNG", error))?;
    if format == ImageFormat::Png {
        return Ok(png.to_vec());
    }
    let mut rgba = image.to_rgba8();
    for pixel in rgba.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let over =
            |c: u8| ((u32::from(c) * u32::from(a) + 255 * (255 - u32::from(a)) + 127) / 255) as u8;
        pixel.0 = [over(r), over(g), over(b), 255];
    }
    let rgb = image::DynamicImage::ImageRgba8(rgba).to_rgb8();
    let mut bytes = Cursor::new(Vec::new());
    rgb.write_to(&mut bytes, format)
        .map_err(|error| CoreError::format("image", error))?;
    Ok(bytes.into_inner())
}

/// Writes the base64 PNG `data` to `path` in the format its extension names.
///
/// # Errors
/// The errors of [`decode_base64`] and [`convert`], and [`CoreError::Io`]
/// when the file cannot be written.
pub fn save(data: &str, path: &FsPath) -> Result<(), CoreError> {
    let png = decode_base64(data)?;
    let bytes = convert(&png, format_of_path(path))?;
    std::fs::write(path, bytes).map_err(|error| CoreError::io(path.display(), error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, Rgba};

    fn encode_base64(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk.len();
            let v = (u32::from(chunk[0]) << 16)
                | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
                | u32::from(*chunk.get(2).unwrap_or(&0));
            for k in 0..4 {
                if k <= n {
                    out.push(ALPHABET[((v >> (18 - 6 * k)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn png() -> Vec<u8> {
        let buffer: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(4, 2, |x, _| {
            Rgba([200, 100, 50, if x == 0 { 0 } else { 255 }])
        });
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(buffer)
            .write_to(&mut bytes, ImageFormat::Png)
            .expect("encodes");
        bytes.into_inner()
    }

    #[test]
    fn base64_round_trips() {
        for sample in [&b""[..], b"a", b"ab", b"abc", b"abcd", &[0, 255, 128, 7, 9]] {
            assert_eq!(
                decode_base64(&encode_base64(sample)).expect("decodes"),
                sample
            );
        }
        let data = format!("data:image/png;base64,{}", encode_base64(b"hello"));
        assert_eq!(decode_base64(&data).expect("decodes"), b"hello");
        assert!(decode_base64("ab!c").is_err());
        assert!(decode_base64("abcde").is_err());
    }

    #[test]
    fn formats_follow_the_extension() {
        assert_eq!(format_of_path(FsPath::new("a/view.JPG")), ImageFormat::Jpeg);
        assert_eq!(format_of_path(FsPath::new("view.bmp")), ImageFormat::Bmp);
        assert_eq!(format_of_path(FsPath::new("view.Gif")), ImageFormat::Gif);
        assert_eq!(format_of_path(FsPath::new("view.png")), ImageFormat::Png);
        assert_eq!(format_of_path(FsPath::new("view")), ImageFormat::Png);
    }

    #[test]
    fn images_are_written_in_each_format() {
        let dir = std::env::temp_dir().join("carve_view_image_test");
        std::fs::create_dir_all(&dir).expect("dir");
        let data = encode_base64(&png());
        for (name, format) in [
            ("v.png", ImageFormat::Png),
            ("v.jpg", ImageFormat::Jpeg),
            ("v.bmp", ImageFormat::Bmp),
            ("v.gif", ImageFormat::Gif),
        ] {
            let path = dir.join(name);
            save(&data, &path).expect("saves");
            let bytes = std::fs::read(&path).expect("reads");
            assert_eq!(image::guess_format(&bytes).expect("known"), format);
            let back = image::load_from_memory(&bytes).expect("decodes").to_rgba8();
            assert_eq!(back.dimensions(), (4, 2));
            if matches!(format, ImageFormat::Bmp | ImageFormat::Gif) {
                // The transparent column lies on white; the rest keeps its colour.
                assert_eq!(back.get_pixel(0, 0).0, [255, 255, 255, 255]);
                assert_eq!(back.get_pixel(1, 0).0, [200, 100, 50, 255]);
            }
        }
        assert!(save("bm90IGEgcG5n", &dir.join("bad.png")).is_err());
    }
}
