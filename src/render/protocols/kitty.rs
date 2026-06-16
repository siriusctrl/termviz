use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::{Compression, write::ZlibEncoder};
use image::{DynamicImage, RgbaImage};
use std::io::Write;

use crate::render::protocols::{
    chunked_base64_payload, png_chunked_base64, png_chunked_base64_rgba,
};

const KITTY_PREFIX: &str = "\u{1b}_G";
const KITTY_SUFFIX: &str = "\u{1b}\\";

pub(crate) fn render(image: &DynamicImage) -> Result<String> {
    render_with_png_chunks(image)
}

pub(crate) fn render_for_size(image: &DynamicImage, columns: u32, rows: u32) -> Result<String> {
    render_with_png_chunks_for_size(image, Some((columns.max(1), rows.max(1))))
}

pub(crate) fn render_rgba_for_size(image: &RgbaImage, columns: u32, rows: u32) -> Result<String> {
    render_with_rgba_png_chunks_for_size(image, Some((columns.max(1), rows.max(1))))
}

pub(crate) fn render_rgba_zlib_for_size(
    image: &RgbaImage,
    columns: u32,
    rows: u32,
) -> Result<String> {
    render_with_rgba_zlib_chunks_for_size(image, Some((columns.max(1), rows.max(1))))
}

fn render_with_png_chunks(image: &DynamicImage) -> Result<String> {
    render_with_png_chunks_for_size(image, None)
}

fn render_with_png_chunks_for_size(
    image: &DynamicImage,
    display_cells: Option<(u32, u32)>,
) -> Result<String> {
    let chunks = png_chunked_base64(image).context("encoding kitty image payload")?;
    render_chunked_payload(image.width(), image.height(), display_cells, &chunks)
}

fn render_with_rgba_png_chunks_for_size(
    image: &RgbaImage,
    display_cells: Option<(u32, u32)>,
) -> Result<String> {
    let chunks = png_chunked_base64_rgba(image).context("encoding kitty image payload")?;
    render_chunked_payload(image.width(), image.height(), display_cells, &chunks)
}

fn render_with_rgba_zlib_chunks_for_size(
    image: &RgbaImage,
    display_cells: Option<(u32, u32)>,
) -> Result<String> {
    let compressed =
        zlib_compress_bytes(image.as_raw()).context("compressing kitty RGBA payload")?;
    let encoded = STANDARD.encode(compressed);
    render_chunked_payload_str_with_format(
        image.width(),
        image.height(),
        display_cells,
        &encoded,
        "f=32,o=z",
    )
}

fn zlib_compress_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
}

fn render_chunked_payload(
    width: u32,
    height: u32,
    display_cells: Option<(u32, u32)>,
    chunks: &[String],
) -> Result<String> {
    render_chunked_payload_with_format(width, height, display_cells, chunks, "f=100")
}

fn render_chunked_payload_with_format(
    width: u32,
    height: u32,
    display_cells: Option<(u32, u32)>,
    chunks: &[String],
    format_control: &str,
) -> Result<String> {
    if chunks.is_empty() {
        return Ok(format!("{KITTY_PREFIX}f=100,t=d,m=0;\u{1b}\\"));
    }

    let payload_len = chunks.iter().map(String::len).sum::<usize>();
    let mut output = String::with_capacity(payload_len + chunks.len() * 64);

    for (index, chunk) in chunks.iter().enumerate() {
        let is_last = index + 1 == chunks.len();
        let chunk_mode = if is_last { 0 } else { 1 };

        output.push_str(KITTY_PREFIX);
        if index == 0 {
            let display = display_cells
                .map(|(columns, rows)| format!(",c={columns},r={rows},C=1"))
                .unwrap_or_default();
            output.push_str(&format!(
                "a=T,{format_control},t=d,s={width},v={height}{display},m={chunk_mode};{chunk}",
                chunk = chunk
            ));
        } else {
            output.push_str(&format!("m={chunk_mode};{chunk}", chunk = chunk));
        }
        output.push_str(KITTY_SUFFIX);
    }

    Ok(output)
}

fn render_chunked_payload_str_with_format(
    width: u32,
    height: u32,
    display_cells: Option<(u32, u32)>,
    base64_payload: &str,
    format_control: &str,
) -> Result<String> {
    let chunks = chunked_base64_payload(base64_payload, 4096);
    if chunks.is_empty() {
        return Ok(format!("{KITTY_PREFIX}f=100,t=d,m=0;\u{1b}\\"));
    }

    let mut output = String::with_capacity(base64_payload.len() + chunks.len() * 64);
    for (index, chunk) in chunks.iter().enumerate() {
        let is_last = index + 1 == chunks.len();
        let chunk_mode = if is_last { 0 } else { 1 };

        output.push_str(KITTY_PREFIX);
        if index == 0 {
            let display = display_cells
                .map(|(columns, rows)| format!(",c={columns},r={rows},C=1"))
                .unwrap_or_default();
            output.push_str(&format!(
                "a=T,{format_control},t=d,s={width},v={height}{display},m={chunk_mode};{chunk}",
                chunk = chunk
            ));
        } else {
            output.push_str(&format!("m={chunk_mode};{chunk}", chunk = chunk));
        }
        output.push_str(KITTY_SUFFIX);
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgba};

    #[test]
    fn kitty_payload_has_valid_prefix_and_suffix() {
        let image =
            image::DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 2, Rgba([0, 0, 0, 255])));
        let payload = super::render(&image).unwrap();
        assert!(payload.starts_with("\u{1b}_G"));
        assert!(payload.ends_with("\u{1b}\\"));
        assert!(payload.contains("t=d"));
    }

    #[test]
    fn kitty_payload_can_request_display_cell_size() {
        let image =
            image::DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 2, Rgba([0, 0, 0, 255])));
        let payload = super::render_for_size(&image, 80, 24).unwrap();
        assert!(payload.contains("t=d"));
        assert!(payload.contains(",c=80,r=24,C=1,"));
    }

    #[test]
    fn kitty_multichunk_payload_uses_continuation_headers_only_after_first_chunk() {
        let image = image::DynamicImage::ImageRgba8(ImageBuffer::from_fn(96, 96, |x, y| {
            let r = ((x * 13 + y * 7) % 251) as u8;
            let g = ((x * 5 + y * 17) % 253) as u8;
            let b = ((x * 29 + y * 3) % 255) as u8;
            Rgba([r, g, b, 255])
        }));

        let payload = super::render(&image).unwrap();
        let packets = payload
            .split(super::KITTY_PREFIX)
            .filter_map(|packet| packet.strip_suffix(super::KITTY_SUFFIX))
            .collect::<Vec<_>>();

        assert!(packets.len() > 1);
        assert!(packets[0].starts_with("a=T,f=100,t=d,"));
        for packet in packets.iter().skip(1) {
            assert!(packet.starts_with("m="));
            assert!(!packet.contains("a=T"));
            let (_, data) = packet.split_once(';').unwrap();
            assert!(data.len() <= 4096);
        }
    }
}
