use anyhow::{Context, Result};

use crate::render::protocols::png_chunked_base64;
use image::DynamicImage;

const KITTY_PREFIX: &str = "\u{1b}_G";
const KITTY_SUFFIX: &str = "\u{1b}\\";

pub(crate) fn render(image: &DynamicImage) -> Result<String> {
    render_with_png_chunks(image)
}

pub(crate) fn render_for_size(image: &DynamicImage, columns: u32, rows: u32) -> Result<String> {
    render_with_png_chunks_for_size(image, Some((columns.max(1), rows.max(1))))
}

fn render_with_png_chunks(image: &DynamicImage) -> Result<String> {
    render_with_png_chunks_for_size(image, None)
}

fn render_with_png_chunks_for_size(
    image: &DynamicImage,
    display_cells: Option<(u32, u32)>,
) -> Result<String> {
    let chunks = png_chunked_base64(image).context("encoding kitty image payload")?;
    if chunks.is_empty() {
        return Ok(format!("{KITTY_PREFIX}f=100,t=d,m=0;\u{1b}\\"));
    }

    let width = image.width();
    let height = image.height();
    let mut output = String::new();

    for (index, chunk) in chunks.iter().enumerate() {
        let is_last = index + 1 == chunks.len();
        let chunk_mode = if is_last { 0 } else { 1 };

        output.push_str(KITTY_PREFIX);
        if index == 0 {
            let display = display_cells
                .map(|(columns, rows)| format!(",c={columns},r={rows}"))
                .unwrap_or_default();
            output.push_str(&format!(
                "a=T,f=100,t=d,s={width},v={height}{display},m={chunk_mode};{chunk}",
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
        assert!(payload.contains(",c=80,r=24,"));
    }

    #[test]
    fn kitty_multichunk_payload_uses_continuation_headers_only_after_first_chunk() {
        let image = image::DynamicImage::ImageRgba8(ImageBuffer::from_fn(96, 96, |x, y| {
            let r = ((x * 13 + y * 7) % 251) as u8;
            let g = ((x * 5 + y * 17) % 253) as u8;
            let b = ((x * 29 + y * 3) % 255) as u8;
            Rgba([r, g, b, 255])
        }));

        let payload = super::render_for_size(&image, 80, 24).unwrap();
        let packets = payload
            .split(super::KITTY_PREFIX)
            .filter_map(|packet| packet.strip_suffix(super::KITTY_SUFFIX))
            .collect::<Vec<_>>();

        assert!(packets.len() > 1);
        assert!(packets[0].starts_with("a=T,f=100,t=d,"));
        assert!(packets[0].contains(",c=80,r=24,"));
        for packet in packets.iter().skip(1) {
            assert!(packet.starts_with("m="));
            assert!(!packet.contains("a=T"));
            assert!(!packet.contains("c=80"));
            let (_, data) = packet.split_once(';').unwrap();
            assert!(data.len() <= 4096);
        }
    }
}
