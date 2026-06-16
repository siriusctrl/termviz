use std::{fs, sync::OnceLock};

use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use image::{Rgba, RgbaImage};

pub(super) const BASE_GLYPH_WIDTH: i32 = 5;
pub(super) const BASE_GLYPH_HEIGHT: i32 = 7;
pub(super) const BASE_TEXT_ADVANCE: i32 = 6;

#[derive(Debug, Clone, Copy)]
pub(super) struct TextMetrics {
    scale: i32,
}

impl TextMetrics {
    pub(super) fn new(scale: u32) -> Self {
        Self {
            scale: i32::try_from(scale.max(1)).unwrap_or(i32::MAX),
        }
    }

    pub(super) fn scale(&self) -> i32 {
        self.scale
    }

    pub(super) fn glyph_width(&self) -> i32 {
        BASE_GLYPH_WIDTH.saturating_mul(self.scale)
    }

    pub(super) fn glyph_height(&self) -> i32 {
        BASE_GLYPH_HEIGHT.saturating_mul(self.scale)
    }

    pub(super) fn advance(&self) -> i32 {
        BASE_TEXT_ADVANCE.saturating_mul(self.scale)
    }

    pub(super) fn width(&self, text: &str) -> i32 {
        let chars = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);
        chars
            .saturating_mul(self.advance())
            .saturating_sub(self.advance().saturating_sub(self.glyph_width()))
    }
}

pub(super) fn draw_text(
    image: &mut RgbaImage,
    metrics: TextMetrics,
    x: i32,
    y: i32,
    text: &str,
    color: Rgba<u8>,
) {
    let max_width = i32::try_from(image.width())
        .unwrap_or(i32::MAX)
        .saturating_sub(x.max(0));
    draw_text_clipped(image, metrics, x, y, text, color, max_width);
}

pub(super) fn draw_text_clipped(
    image: &mut RgbaImage,
    metrics: TextMetrics,
    x: i32,
    y: i32,
    text: &str,
    color: Rgba<u8>,
    max_width: i32,
) {
    if draw_antialiased_text_clipped(image, metrics, x, y, text, color, max_width) {
        return;
    }
    draw_bitmap_text_clipped(image, metrics, x, y, text, color, max_width);
}

fn draw_antialiased_text_clipped(
    image: &mut RgbaImage,
    metrics: TextMetrics,
    x: i32,
    y: i32,
    text: &str,
    color: Rgba<u8>,
    max_width: i32,
) -> bool {
    let Some(font) = system_monospace_font() else {
        return false;
    };
    if max_width < metrics.glyph_width() {
        return true;
    }

    let mut cursor_x = x.max(0);
    let mut cursor_y = y;
    let line_start = x.max(0);
    let right_limit = line_start.saturating_add(max_width);
    let scale = PxScale::from(((metrics.glyph_height() as f32) * 1.35).max(9.0));
    let scaled_font = font.as_scaled(scale);
    let baseline_offset = (metrics.glyph_height() as f32 * 1.08).max(8.0);

    for ch in text.chars() {
        if ch == '\n' {
            cursor_y += metrics.glyph_height() + metrics.scale();
            cursor_x = line_start;
            continue;
        }

        let glyph_id = font.glyph_id(ch);
        let advance = scaled_font.h_advance(glyph_id).ceil().max(1.0) as i32;
        if cursor_x.saturating_add(advance) > right_limit {
            break;
        }

        let glyph = glyph_id.with_scale_and_position(
            scale,
            point(cursor_x as f32, cursor_y as f32 + baseline_offset),
        );
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|glyph_x, glyph_y, coverage| {
                let pixel_x = bounds.min.x.floor() as i32 + glyph_x as i32;
                let pixel_y = bounds.min.y.floor() as i32 + glyph_y as i32;
                blend_pixel_if_in_bounds(image, pixel_x, pixel_y, color, coverage);
            });
        }
        cursor_x = cursor_x.saturating_add(advance);
    }

    true
}

fn system_monospace_font() -> Option<&'static FontArc> {
    static FONT: OnceLock<Option<FontArc>> = OnceLock::new();
    FONT.get_or_init(load_system_monospace_font).as_ref()
}

fn load_system_monospace_font() -> Option<FontArc> {
    const FONT_PATHS: &[&str] = &[
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansMono-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
    ];

    FONT_PATHS.iter().find_map(|path| {
        let bytes = fs::read(path).ok()?;
        FontArc::try_from_vec(bytes).ok()
    })
}

fn draw_bitmap_text_clipped(
    image: &mut RgbaImage,
    metrics: TextMetrics,
    x: i32,
    y: i32,
    text: &str,
    color: Rgba<u8>,
    max_width: i32,
) {
    if max_width < metrics.glyph_width() {
        return;
    }

    let mut cursor_x = x.max(0);
    let mut cursor_y = y;
    let line_start = x.max(0);
    let right_limit = line_start.saturating_add(max_width);
    for ch in text.chars() {
        if ch == '\n' {
            cursor_y += metrics.glyph_height() + metrics.scale();
            cursor_x = line_start;
            continue;
        }

        if cursor_x.saturating_add(metrics.glyph_width()) > right_limit {
            break;
        }
        let glyph = glyph(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..BASE_GLYPH_WIDTH {
                if (bits >> (BASE_GLYPH_WIDTH - 1 - col)) & 1 == 0 {
                    continue;
                }
                for sy in 0..metrics.scale() {
                    for sx in 0..metrics.scale() {
                        set_pixel_if_in_bounds(
                            image,
                            cursor_x + col * metrics.scale() + sx,
                            cursor_y + row as i32 * metrics.scale() + sy,
                            color,
                        );
                    }
                }
            }
        }
        cursor_x = cursor_x.saturating_add(metrics.advance());
    }
}

fn blend_pixel_if_in_bounds(image: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>, coverage: f32) {
    if x < 0 || y < 0 {
        return;
    }
    let width = i32::try_from(image.width()).unwrap_or(i32::MAX);
    let height = i32::try_from(image.height()).unwrap_or(i32::MAX);
    if x >= width || y >= height {
        return;
    }

    let dst = image.get_pixel(x as u32, y as u32);
    let src_alpha = (f32::from(color[3]) / 255.0) * coverage.clamp(0.0, 1.0);
    let inv_alpha = 1.0 - src_alpha;
    let blended = Rgba([
        blend_channel(color[0], dst[0], src_alpha, inv_alpha),
        blend_channel(color[1], dst[1], src_alpha, inv_alpha),
        blend_channel(color[2], dst[2], src_alpha, inv_alpha),
        255,
    ]);
    image.put_pixel(x as u32, y as u32, blended);
}

fn blend_channel(src: u8, dst: u8, src_alpha: f32, inv_alpha: f32) -> u8 {
    (f32::from(src) * src_alpha + f32::from(dst) * inv_alpha)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn set_pixel_if_in_bounds(image: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    if x < 0 || y < 0 {
        return;
    }
    let width = i32::try_from(image.width()).unwrap_or(i32::MAX);
    let height = i32::try_from(image.height()).unwrap_or(i32::MAX);
    if x < width && y < height {
        image.put_pixel(x as u32, y as u32, color);
    }
}

fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        ' ' => [0, 0, 0, 0, 0, 0, 0],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b01110,
        ],
        '6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
        'A' => [
            0b00100, 0b01010, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => [
            0b00001, 0b00001, 0b00001, 0b00001, 0b10001, 0b10001, 0b01110,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'X' => [
            0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b01010, 0b10001,
        ],
        'Y' => [
            0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '-' => [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
        '.' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100,
        ],
        ',' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100, 0b01000,
        ],
        ':' => [
            0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000,
        ],
        '(' => [
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ],
        ')' => [
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ],
        '[' => [
            0b11110, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11110,
        ],
        ']' => [
            0b01111, 0b00001, 0b00001, 0b00001, 0b00001, 0b00001, 0b01111,
        ],
        '/' => [
            0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000,
        ],
        '+' => [
            0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000,
        ],
        '_' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111,
        ],
        _ => [
            0b10101, 0b11111, 0b01110, 0b11011, 0b11111, 0b10101, 0b10101,
        ],
    }
}
