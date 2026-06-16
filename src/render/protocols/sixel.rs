use anyhow::Result;

use image::{DynamicImage, imageops::FilterType};

const SIXEL_PREFIX: &str = "\u{1b}Pq";
const SIXEL_SUFFIX: &str = "\u{1b}\\";
const CELL_PIXEL_WIDTH: u32 = 8;
const CELL_PIXEL_HEIGHT: u32 = 16;

const GRAYSCALE: [(u8, u8, u8); 4] = [(0, 0, 0), (85, 85, 85), (170, 170, 170), (255, 255, 255)];

pub(crate) fn render(image: &DynamicImage) -> Result<String> {
    render_pixels(image)
}

pub(crate) fn render_for_size(image: &DynamicImage, columns: u32, rows: u32) -> Result<String> {
    let target_width = columns.max(1).saturating_mul(CELL_PIXEL_WIDTH);
    let target_height = rows.max(1).saturating_mul(CELL_PIXEL_HEIGHT);
    let resized = resize_to_fit_pixels(image, target_width, target_height);
    render_pixels(&resized)
}

fn render_pixels(image: &DynamicImage) -> Result<String> {
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return Ok(format!("{SIXEL_PREFIX}{SIXEL_SUFFIX}"));
    }

    let gray = image.to_luma8();
    let mut output = String::from(SIXEL_PREFIX);
    output.push_str(&format!(";1;0;0;0;{width};{height}\r"));

    for row in (0..height).step_by(6) {
        for (color_index, (r, g, b)) in GRAYSCALE.iter().enumerate() {
            let mut sixel_data = String::new();
            for x in 0..width {
                let mut pattern = 0u8;
                for bit in 0..6 {
                    let y = row + bit;
                    if y >= height {
                        continue;
                    }
                    let pixel = gray.get_pixel(x, y).0[0];
                    let level = luminance_index(pixel);
                    if level == color_index as u8 {
                        pattern |= 1 << bit;
                    }
                }
                sixel_data.push((pattern + b'?') as char);
            }

            if sixel_data.chars().any(|ch| ch != '?') {
                output.push_str(&format!("#{};2;{};{};{}", color_index, r, g, b));
                output.push_str(&sixel_data);
            }
        }

        if row + 6 < height {
            output.push('-');
        }
    }

    output.push_str(SIXEL_SUFFIX);
    Ok(output)
}

fn resize_to_fit_pixels(image: &DynamicImage, max_width: u32, max_height: u32) -> DynamicImage {
    if image.width() == 0 || image.height() == 0 {
        return image.clone();
    }

    let width_scale = max_width as f64 / image.width() as f64;
    let height_scale = max_height as f64 / image.height() as f64;
    let scale = width_scale.min(height_scale).max(0.01);
    let width = ((image.width() as f64 * scale).round()).max(1.0) as u32;
    let height = ((image.height() as f64 * scale).round()).max(1.0) as u32;
    image.resize_exact(width, height, FilterType::Triangle)
}

fn luminance_index(pixel: u8) -> u8 {
    match pixel {
        0..=63 => 0,
        64..=127 => 1,
        128..=191 => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgba};

    #[test]
    fn sixel_payload_has_valid_prefix_and_suffix() {
        let image =
            image::DynamicImage::ImageRgba8(ImageBuffer::from_pixel(1, 1, Rgba([0, 0, 0, 255])));
        let payload = super::render(&image).unwrap();
        assert!(payload.starts_with("\u{1b}Pq"));
        assert!(payload.ends_with("\u{1b}\\"));
    }

    #[test]
    fn sixel_payload_can_scale_toward_display_size() {
        let image =
            image::DynamicImage::ImageRgba8(ImageBuffer::from_pixel(1, 1, Rgba([0, 0, 0, 255])));
        let payload = super::render_for_size(&image, 10, 4).unwrap();
        assert!(payload.contains(";64;64\r"));
    }
}
