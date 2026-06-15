use anyhow::{Context, Result};

use crate::render::protocols::png_chunked_base64;
use image::DynamicImage;

const KITTY_PREFIX: &str = "\u{1b}_G";
const KITTY_SUFFIX: &str = "\u{1b}\\";

pub(crate) fn render(image: &DynamicImage) -> Result<String> {
    render_with_png_chunks(image)
}

fn render_with_png_chunks(image: &DynamicImage) -> Result<String> {
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
        output.push_str(&format!(
            "a=T,f=100,t=d,s={width},v={height},m={chunk_mode};{chunk}",
            chunk = chunk
        ));
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
}
