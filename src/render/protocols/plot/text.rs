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
