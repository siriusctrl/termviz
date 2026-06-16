use anyhow::Result;
use image::{DynamicImage, Rgba};

const MAX_ANSI_COLUMNS: u32 = 80;
const MAX_ANSI_ROWS: u32 = 40;

pub(crate) fn render_raster(image: &DynamicImage) -> Result<String> {
    render_raster_for_size(image, MAX_ANSI_COLUMNS, MAX_ANSI_ROWS)
}

pub(crate) fn render_raster_for_size(
    image: &DynamicImage,
    max_columns: u32,
    max_rows: u32,
) -> Result<String> {
    if max_columns == 0 || max_rows == 0 {
        return Ok(String::new());
    }
    let scaled = fit_dimensions(image.width(), image.height(), max_columns, max_rows);
    if scaled.0 == 0 || scaled.1 == 0 {
        return Ok(String::new());
    }

    let rgba = image.to_rgba8();
    let resized = nearest_scaled_rgba(&rgba, scaled.0, scaled.1);
    Ok(render_half_blocks(&resized, scaled.0, scaled.1))
}

pub(crate) fn fit_dimensions(
    source_width: u32,
    source_height: u32,
    max_columns: u32,
    max_rows: u32,
) -> (u32, u32) {
    if source_width == 0 || source_height == 0 {
        return (0, 0);
    }

    let max_pixel_rows = (max_rows * 2).max(1);
    let width_scale = max_columns as f64 / source_width as f64;
    let height_scale = max_pixel_rows as f64 / source_height as f64;
    let scale = width_scale.min(height_scale).clamp(0.01, 1.0);

    let scaled_width = ((source_width as f64 * scale).round()).max(1.0) as u32;
    let scaled_height = ((source_height as f64 * scale).round()).max(1.0) as u32;
    (scaled_width, scaled_height)
}

fn nearest_scaled_rgba(
    source: &image::ImageBuffer<Rgba<u8>, Vec<u8>>,
    width: u32,
    height: u32,
) -> Vec<Rgba<u8>> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let source_width = source.width() as f64;
    let source_height = source.height() as f64;
    let mut pixels = Vec::with_capacity((width * height) as usize);

    for y in 0..height {
        let source_y = ((y as f64 / height as f64) * source_height) as u32;
        let source_y = source_y.min(source.height().saturating_sub(1));
        for x in 0..width {
            let source_x = ((x as f64 / width as f64) * source_width) as u32;
            let source_x = source_x.min(source.width().saturating_sub(1));
            pixels.push(*source.get_pixel(source_x, source_y));
        }
    }
    pixels
}

fn render_half_blocks(pixels: &[Rgba<u8>], width: u32, height: u32) -> String {
    let mut lines = String::new();
    let width_usize = width as usize;
    let mut row = 0usize;
    while row < height as usize {
        for col in 0..width_usize {
            let upper = pixel_at(pixels, width_usize, col, row);
            let lower = if row + 1 < height as usize {
                pixel_at(pixels, width_usize, col, row + 1)
            } else {
                upper
            };
            append_half_block(&mut lines, upper, lower);
        }
        lines.push_str("\x1b[0m\n");
        row += 2;
    }
    lines
}

fn pixel_at(pixels: &[Rgba<u8>], width: usize, x: usize, y: usize) -> Rgba<u8> {
    let index = y.saturating_mul(width).saturating_add(x);
    pixels.get(index).copied().unwrap_or(Rgba([0, 0, 0, 255]))
}

fn append_half_block(out: &mut String, upper: Rgba<u8>, lower: Rgba<u8>) {
    let [ur, ug, ub, ua] = upper.0;
    let [lr, lg, lb, la] = lower.0;
    let upper_visible = ua >= 10;
    let lower_visible = la >= 10;

    if !upper_visible && !lower_visible {
        out.push(' ');
        return;
    }

    if upper_visible && !lower_visible {
        out.push_str(&format!("\x1b[38;2;{};{};{}m▀", ur, ug, ub));
        return;
    }

    if !upper_visible && lower_visible {
        out.push_str(&format!("\x1b[38;2;{};{};{}m▄", lr, lg, lb));
        return;
    }

    out.push_str(&format!(
        "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m▀",
        ur, ug, ub, lr, lg, lb
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn half_block_keeps_lower_visible_pixel_when_upper_is_transparent() {
        let mut image = RgbaImage::from_pixel(1, 2, Rgba([0, 0, 0, 0]));
        image.put_pixel(0, 1, Rgba([255, 0, 0, 255]));

        let output =
            render_raster_for_size(&DynamicImage::ImageRgba8(image), 1, 1).expect("render raster");

        assert!(output.contains('▄'));
        assert!(output.contains("38;2;255;0;0"));
    }

    #[test]
    fn half_block_leaves_fully_transparent_pixels_blank() {
        let image = RgbaImage::from_pixel(1, 2, Rgba([0, 0, 0, 0]));

        let output =
            render_raster_for_size(&DynamicImage::ImageRgba8(image), 1, 1).expect("render raster");

        assert_eq!(output, " \x1b[0m\n");
    }
}
