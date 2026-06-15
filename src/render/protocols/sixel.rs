use anyhow::Result;

use image::DynamicImage;

const SIXEL_PREFIX: &str = "\u{1b}Pq";
const SIXEL_SUFFIX: &str = "\u{1b}\\";

const GRAYSCALE: [(u8, u8, u8); 4] = [(0, 0, 0), (85, 85, 85), (170, 170, 170), (255, 255, 255)];

pub(crate) fn render(image: &DynamicImage) -> Result<String> {
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
}
