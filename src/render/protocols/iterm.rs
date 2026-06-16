use anyhow::{Result, anyhow};

use crate::render::protocols::encode_png_base64;
use image::DynamicImage;

const ITERM_PREFIX: &str = "\u{1b}]1337;File";
const ITERM_SUFFIX: &str = "\u{07}";

pub(crate) fn render(image: &DynamicImage) -> Result<String> {
    render_with_display_size(image, image.width(), image.height())
}

pub(crate) fn render_for_size(image: &DynamicImage, columns: u32, rows: u32) -> Result<String> {
    render_with_display_size(image, columns.max(1), rows.max(1))
}

fn render_with_display_size(
    image: &DynamicImage,
    display_width: u32,
    display_height: u32,
) -> Result<String> {
    let payload =
        encode_png_base64(image).map_err(|error| anyhow!("encoding image payload: {error}"))?;

    Ok(format!(
        "{ITERM_PREFIX}=inline=1;width={display_width};height={display_height};preserveAspectRatio=1;size={}:{payload}{ITERM_SUFFIX}",
        payload.len()
    ))
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgba};

    #[test]
    fn iterm_payload_has_valid_prefix_and_suffix() {
        let image = image::DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            1,
            1,
            Rgba([255, 255, 255, 255]),
        ));
        let payload = super::render(&image).unwrap();
        assert!(payload.starts_with("\u{1b}]1337;File=inline=1"));
        assert!(payload.ends_with("\u{7}"));
    }

    #[test]
    fn iterm_payload_can_request_display_cell_size() {
        let image = image::DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            1,
            1,
            Rgba([255, 255, 255, 255]),
        ));
        let payload = super::render_for_size(&image, 80, 24).unwrap();
        assert!(payload.contains("width=80;height=24;"));
    }
}
