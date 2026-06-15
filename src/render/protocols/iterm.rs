use anyhow::{Result, anyhow};

use crate::render::protocols::encode_png_base64;
use image::DynamicImage;

const ITERM_PREFIX: &str = "\u{1b}]1337;File";
const ITERM_SUFFIX: &str = "\u{07}";

pub(crate) fn render(image: &DynamicImage) -> Result<String> {
    let width = image.width();
    let height = image.height();
    let payload =
        encode_png_base64(image).map_err(|error| anyhow!("encoding image payload: {error}"))?;

    Ok(format!(
        "{ITERM_PREFIX}=inline=1;width={width};height={height};preserveAspectRatio=0:{payload}{ITERM_SUFFIX}"
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
}
