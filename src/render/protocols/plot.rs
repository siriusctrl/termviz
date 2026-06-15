use anyhow::{Context, Result};
use image::{DynamicImage, Rgba, RgbaImage};

use crate::plot::{
    PlotKind,
    model::{PlotBounds, PlotPoint, PlotScene},
};

const PLOT_CANVAS_WIDTH: u32 = 640;
const PLOT_CANVAS_HEIGHT: u32 = 360;
const LEFT_MARGIN: f64 = 74.0;
const RIGHT_MARGIN: f64 = 18.0;
const TOP_MARGIN: f64 = 78.0;
const BOTTOM_MARGIN: f64 = 50.0;
const MARK_RADIUS: i32 = 2;
const AXIS_COLOR: Rgba<u8> = Rgba([42, 42, 42, 255]);
const GRID_COLOR: Rgba<u8> = Rgba([228, 228, 228, 255]);
const TEXT_COLOR: Rgba<u8> = Rgba([24, 24, 24, 255]);
const TITLE_COLOR: Rgba<u8> = Rgba([40, 40, 40, 255]);
const BACKGROUND_COLOR: Rgba<u8> = Rgba([255, 255, 255, 255]);
const GLYPH_WIDTH: i32 = 5;
const GLYPH_HEIGHT: i32 = 7;
const TEXT_ADVANCE: i32 = 6;
const HEADER_PADDING: i32 = 12;
const TITLE_Y: i32 = 8;
const LEGEND_TOP: i32 = 24;
const LEGEND_MAX_ROWS: usize = 4;
const LEGEND_MIN_WIDTH: i32 = 126;
const LEGEND_MAX_WIDTH: i32 = 220;
const LEGEND_ROW_HEIGHT: i32 = 10;
const LEGEND_SWATCH_WIDTH: i32 = 14;
const LEGEND_TEXT_GAP: i32 = 6;
const HEADER_GAP: i32 = 8;
const Y_TICK_GAP: i32 = 8;
const CHART_STROKE: [[u8; 4]; 6] = [
    [31, 119, 180, 255],
    [255, 127, 14, 255],
    [44, 160, 44, 255],
    [214, 39, 40, 255],
    [148, 103, 189, 255],
    [140, 86, 75, 255],
];

#[derive(Debug, Clone, Copy)]
struct PlotArea {
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Copy)]
struct LegendArea {
    left: i32,
    width: i32,
}

pub(crate) fn render_plot(scene: &PlotScene, kind: PlotKind) -> Result<DynamicImage> {
    let bounds = scene.bounds().context("plot scene is empty")?.normalized();
    render_plot_for_bounds(scene, kind, bounds)
}

pub(crate) fn render_plot_for_bounds(
    scene: &PlotScene,
    kind: PlotKind,
    viewport: PlotBounds,
) -> Result<DynamicImage> {
    let mut image = RgbaImage::from_pixel(PLOT_CANVAS_WIDTH, PLOT_CANVAS_HEIGHT, BACKGROUND_COLOR);

    if scene.series.is_empty() {
        return Ok(DynamicImage::ImageRgba8(image));
    }

    let bounds = viewport.normalized();
    let area = chart_geometry();
    draw_frame(&mut image, area);
    draw_grid_lines(&mut image, area);
    draw_axes(&mut image, area, bounds);
    draw_axis_labels(&mut image, area);

    for (series_index, series) in scene.series.iter().enumerate() {
        let color = Rgba(CHART_STROKE[series_index % CHART_STROKE.len()]);
        match kind {
            PlotKind::Line => {
                for pair in series.points.windows(2) {
                    let Some((start, end)) = clip_to_bounds(&pair[0], &pair[1], bounds) else {
                        continue;
                    };
                    let (x0, y0) = map_point(&start, bounds, &area);
                    let (x1, y1) = map_point(&end, bounds, &area);
                    draw_line(&mut image, x0, y0, x1, y1, color);
                }

                let visible: Vec<_> = series
                    .points
                    .iter()
                    .filter(|point| is_point_in_bounds(point, &bounds))
                    .map(|point| map_point(point, bounds, &area))
                    .collect();
                if let Some(&(x, y)) = visible.first() {
                    draw_dot(&mut image, x, y, MARK_RADIUS, color);
                }
                if let Some(&(x, y)) = visible.last() {
                    draw_dot(&mut image, x, y, MARK_RADIUS, color);
                }
            }
            PlotKind::Scatter => {
                let mapped: Vec<_> = series
                    .points
                    .iter()
                    .filter(|point| is_point_in_bounds(point, &bounds))
                    .map(|point| map_point(point, bounds, &area))
                    .collect();
                for (x, y) in mapped {
                    draw_dot(&mut image, x, y, MARK_RADIUS, color);
                }
            }
        }
    }

    draw_title(&mut image, scene);
    draw_legend(&mut image, scene);
    Ok(DynamicImage::ImageRgba8(image))
}

fn chart_geometry() -> PlotArea {
    let width = f64::from(PLOT_CANVAS_WIDTH) - LEFT_MARGIN - RIGHT_MARGIN;
    let height = f64::from(PLOT_CANVAS_HEIGHT) - TOP_MARGIN - BOTTOM_MARGIN;
    let left = LEFT_MARGIN.max(0.0) as i32;
    let right = (LEFT_MARGIN + width).round() as i32;
    let top = TOP_MARGIN.max(0.0) as i32;
    let bottom = (TOP_MARGIN + height).round() as i32;

    PlotArea {
        left,
        right,
        top,
        bottom,
        width: width.max(1.0),
        height: height.max(1.0),
    }
}

fn draw_frame(image: &mut RgbaImage, area: PlotArea) {
    for x in area.left..=area.right {
        set_pixel_if_in_bounds(image, x, area.top, AXIS_COLOR);
        set_pixel_if_in_bounds(image, x, area.bottom, AXIS_COLOR);
    }

    for y in area.top..=area.bottom {
        set_pixel_if_in_bounds(image, area.left, y, AXIS_COLOR);
        set_pixel_if_in_bounds(image, area.right, y, AXIS_COLOR);
    }
}

fn draw_grid_lines(image: &mut RgbaImage, area: PlotArea) {
    let x_steps = 4;
    let y_steps = 4;
    for step in 1..x_steps {
        let x = area.left + ((area.width * (step as f64) / x_steps as f64) as i32);
        for y in area.top + 1..area.bottom {
            if y % 4 == 0 {
                set_pixel_if_in_bounds(image, x, y, GRID_COLOR);
            }
        }
    }

    for step in 1..y_steps {
        let y = area.top + ((area.height * (step as f64) / y_steps as f64) as i32);
        for x in area.left + 1..area.right {
            if x % 4 == 0 {
                set_pixel_if_in_bounds(image, x, y, GRID_COLOR);
            }
        }
    }
}

fn draw_axes(image: &mut RgbaImage, area: PlotArea, bounds: PlotBounds) {
    let y_span = (bounds.y_max - bounds.y_min).abs().max(f64::EPSILON);
    let x_span = (bounds.x_max - bounds.x_min).abs().max(f64::EPSILON);

    let x_tick_count = 5;
    for step in 0..x_tick_count {
        let ratio = step as f64 / (x_tick_count - 1) as f64;
        let x = (area.left as f64 + area.width * ratio).round() as i32;
        let value = bounds.x_min + x_span * ratio;
        let label = format_axis_label(value);
        let label_width = text_pixel_width(&label);
        let label_x = (x - label_width / 2)
            .max(area.left)
            .min(area.right.saturating_sub(label_width));
        set_pixel_if_in_bounds(
            image,
            x.min(area.right),
            area.bottom.saturating_sub(2),
            AXIS_COLOR,
        );
        draw_text(image, label_x, area.bottom + 8, &label, TEXT_COLOR);
    }

    let y_tick_count = 5;
    for step in 0..y_tick_count {
        let ratio = step as f64 / (y_tick_count - 1) as f64;
        let y = (area.top as f64 + area.height * ratio).round() as i32;
        let value = bounds.y_max - y_span * ratio;
        let label = format_axis_label(value);
        let label_width = text_pixel_width(&label);
        let label_x = area
            .left
            .saturating_sub(Y_TICK_GAP)
            .saturating_sub(label_width)
            .max(2);
        set_pixel_if_in_bounds(image, area.left + 1, y.min(area.bottom), AXIS_COLOR);
        draw_text(image, label_x, y.saturating_sub(3), &label, TEXT_COLOR);
    }
}

fn draw_axis_labels(image: &mut RgbaImage, area: PlotArea) {
    draw_text(
        image,
        area.left + (area.width / 2.0).round() as i32,
        area.bottom + 28,
        "x",
        TEXT_COLOR,
    );
    draw_text(
        image,
        area.left.saturating_sub(18),
        area.top.saturating_sub(GLYPH_HEIGHT + 5),
        "y",
        TEXT_COLOR,
    );
}

fn draw_title(image: &mut RgbaImage, scene: &PlotScene) {
    let title = scene.title.as_deref().unwrap_or("plot");
    let summary = format!(
        "{}  points: {}  series: {}",
        title,
        scene.total_points(),
        scene.series.len(),
    );
    let legend = legend_area(scene);
    let max_width = legend.left.saturating_sub(HEADER_PADDING * 2);
    draw_text_clipped(
        image,
        HEADER_PADDING,
        TITLE_Y,
        &summary,
        TITLE_COLOR,
        max_width,
    );
}

fn draw_legend(image: &mut RgbaImage, scene: &PlotScene) {
    let legend = legend_area(scene);
    for (index, series) in scene.series.iter().take(LEGEND_MAX_ROWS).enumerate() {
        let color = Rgba(CHART_STROKE[index % CHART_STROKE.len()]);
        let row_y = LEGEND_TOP + index as i32 * LEGEND_ROW_HEIGHT;
        let swatch_y = row_y + GLYPH_HEIGHT / 2;
        for pixel in 0..LEGEND_SWATCH_WIDTH {
            set_pixel_if_in_bounds(image, legend.left + pixel, swatch_y, color);
        }
        draw_dot(
            image,
            legend.left + LEGEND_SWATCH_WIDTH / 2,
            swatch_y,
            1,
            color,
        );

        let label = legend_label(index, series);
        draw_text_clipped(
            image,
            legend.left + LEGEND_SWATCH_WIDTH + LEGEND_TEXT_GAP,
            row_y,
            &label,
            color,
            legend
                .width
                .saturating_sub(LEGEND_SWATCH_WIDTH + LEGEND_TEXT_GAP),
        );
    }
}

fn legend_area(scene: &PlotScene) -> LegendArea {
    let widest_label = scene
        .series
        .iter()
        .take(LEGEND_MAX_ROWS)
        .enumerate()
        .map(|(index, series)| text_pixel_width(&legend_label(index, series)))
        .max()
        .unwrap_or(0);
    let width = (LEGEND_SWATCH_WIDTH + LEGEND_TEXT_GAP + widest_label)
        .clamp(LEGEND_MIN_WIDTH, LEGEND_MAX_WIDTH);
    let canvas_width = i32::try_from(PLOT_CANVAS_WIDTH).unwrap_or(i32::MAX);

    LegendArea {
        left: canvas_width
            .saturating_sub(HEADER_PADDING)
            .saturating_sub(width),
        width,
    }
}

fn legend_label(index: usize, series: &crate::plot::model::PlotSeries) -> String {
    let name = if series.name.is_empty() {
        format!("series {}", index + 1)
    } else {
        series.name.clone()
    };
    let label = truncate_chars(&name, 22);
    format!("{label} ({} pts)", series.points.len())
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn draw_dot(image: &mut RgbaImage, x: i32, y: i32, radius: i32, color: Rgba<u8>) {
    if radius <= 0 {
        set_pixel_if_in_bounds(image, x, y, color);
        return;
    }

    let radius_sq = radius * radius;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy > radius_sq {
                continue;
            }
            set_pixel_if_in_bounds(image, x + dx, y + dy, color);
        }
    }
}

fn draw_line(
    image: &mut RgbaImage,
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    color: Rgba<u8>,
) {
    let mut x0 = start_x;
    let mut y0 = start_y;
    let x1 = end_x;
    let y1 = end_y;

    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        set_pixel_if_in_bounds(image, x0, y0, color);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = err * 2;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn map_point(point: &PlotPoint, bounds: PlotBounds, area: &PlotArea) -> (i32, i32) {
    let x_span = (bounds.x_max - bounds.x_min).abs().max(f64::EPSILON);
    let y_span = (bounds.y_max - bounds.y_min).abs().max(f64::EPSILON);
    let x_ratio = ((point.x - bounds.x_min) / x_span).clamp(0.0, 1.0);
    let y_ratio = ((point.y - bounds.y_min) / y_span).clamp(0.0, 1.0);

    let px = f64::from(area.left) + area.width * x_ratio;
    let py = (f64::from(area.top) + area.height) - (area.height * y_ratio);

    (
        px.round()
            .clamp(f64::from(area.left), f64::from(area.right)) as i32,
        py.round()
            .clamp(f64::from(area.top), f64::from(area.bottom)) as i32,
    )
}

fn is_point_in_bounds(point: &PlotPoint, bounds: &PlotBounds) -> bool {
    (bounds.x_min..=bounds.x_max).contains(&point.x)
        && (bounds.y_min..=bounds.y_max).contains(&point.y)
}

fn clip_to_bounds(
    start: &PlotPoint,
    end: &PlotPoint,
    bounds: PlotBounds,
) -> Option<(PlotPoint, PlotPoint)> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let mut t0 = 0.0f64;
    let mut t1 = 1.0f64;

    let update = |p: f64, q: f64, t0: &mut f64, t1: &mut f64| -> bool {
        if p == 0.0 {
            return q >= 0.0;
        }

        let ratio = q / p;
        if p < 0.0 {
            if ratio > *t1 {
                return false;
            }
            *t0 = (*t0).max(ratio);
        } else {
            if ratio < *t0 {
                return false;
            }
            *t1 = (*t1).min(ratio);
        }

        t0 <= t1
    };

    if !update(-dx, start.x - bounds.x_min, &mut t0, &mut t1) {
        return None;
    }
    if !update(dx, bounds.x_max - start.x, &mut t0, &mut t1) {
        return None;
    }
    if !update(-dy, start.y - bounds.y_min, &mut t0, &mut t1) {
        return None;
    }
    if !update(dy, bounds.y_max - start.y, &mut t0, &mut t1) {
        return None;
    }
    if t0 > t1 {
        return None;
    }

    let clipped_start = PlotPoint {
        x: start.x + dx * t0,
        y: start.y + dy * t0,
    };
    let clipped_end = PlotPoint {
        x: start.x + dx * t1,
        y: start.y + dy * t1,
    };
    Some((clipped_start, clipped_end))
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

fn draw_text(image: &mut RgbaImage, x: i32, y: i32, text: &str, color: Rgba<u8>) {
    let max_width = i32::try_from(image.width())
        .unwrap_or(i32::MAX)
        .saturating_sub(x.max(0));
    draw_text_clipped(image, x, y, text, color, max_width);
}

fn draw_text_clipped(
    image: &mut RgbaImage,
    x: i32,
    y: i32,
    text: &str,
    color: Rgba<u8>,
    max_width: i32,
) {
    if max_width < GLYPH_WIDTH {
        return;
    }

    let mut cursor_x = x.max(0);
    let mut cursor_y = y;
    let line_start = x.max(0);
    let right_limit = line_start.saturating_add(max_width);
    for ch in text.chars() {
        if ch == '\n' {
            cursor_y += GLYPH_HEIGHT + 1;
            cursor_x = line_start;
            continue;
        }

        if cursor_x.saturating_add(GLYPH_WIDTH) > right_limit {
            break;
        }
        let glyph = glyph(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if (bits >> (4 - col)) & 1 != 0 {
                    set_pixel_if_in_bounds(image, cursor_x + col, cursor_y + row as i32, color);
                }
            }
        }
        cursor_x = cursor_x.saturating_add(TEXT_ADVANCE);
    }
}

fn text_pixel_width(text: &str) -> i32 {
    let chars = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);
    chars
        .saturating_mul(TEXT_ADVANCE)
        .saturating_sub(TEXT_ADVANCE - GLYPH_WIDTH)
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

fn format_axis_label(value: f64) -> String {
    if value.abs() >= 10_000.0 {
        format!("{value:.1e}")
    } else if value.abs() >= 100.0 {
        format!("{value:.2}")
    } else {
        format!("{value:.3}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plot::model::{PlotPoint, PlotScene, PlotSeries};

    #[test]
    fn render_plot_includes_text_glyph_bytes() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "svc".to_owned(),
                points: vec![PlotPoint { x: 1.0, y: 1.0 }, PlotPoint { x: 2.0, y: 1.5 }],
            }],
        };

        let image = render_plot(&scene, PlotKind::Line).unwrap();
        let bytes = image.to_rgba8();
        assert!(bytes.pixels().any(|pixel| pixel.0 != BACKGROUND_COLOR.0));
    }

    #[test]
    fn render_plot_respects_viewport_bounds() {
        let scene = PlotScene {
            title: Some("zoom".to_owned()),
            series: vec![PlotSeries {
                name: "svc".to_owned(),
                points: vec![PlotPoint { x: 0.0, y: 0.0 }, PlotPoint { x: 10.0, y: 10.0 }],
            }],
        };
        let full = render_plot(&scene, PlotKind::Line).unwrap();
        let zoomed = render_plot_for_bounds(
            &scene,
            PlotKind::Line,
            PlotBounds {
                x_min: 4.0,
                x_max: 6.0,
                y_min: 4.0,
                y_max: 6.0,
            },
        )
        .unwrap();

        assert_ne!(full.to_rgba8().into_raw(), zoomed.to_rgba8().into_raw(),);
    }

    #[test]
    fn plot_layout_keeps_header_legend_outside_chart_area() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: (0..LEGEND_MAX_ROWS)
                .map(|index| PlotSeries {
                    name: format!("service-{index}"),
                    points: vec![PlotPoint {
                        x: index as f64,
                        y: 100.0 + index as f64,
                    }],
                })
                .collect(),
        };

        let area = chart_geometry();
        let legend_bottom = LEGEND_TOP
            + (scene.series.len().min(LEGEND_MAX_ROWS) as i32 - 1) * LEGEND_ROW_HEIGHT
            + GLYPH_HEIGHT;

        assert!(legend_bottom + HEADER_GAP <= area.top);
        assert!(area.left > text_pixel_width("10000.0"));
        assert!(i32::try_from(PLOT_CANVAS_HEIGHT).unwrap() - area.bottom > GLYPH_HEIGHT * 3);
    }

    #[test]
    fn render_plot_line_segment_crosses_viewport_when_endpoints_are_outside() {
        let scene = PlotScene {
            title: None,
            series: vec![PlotSeries {
                name: "crossing".to_owned(),
                points: vec![
                    PlotPoint { x: -5.0, y: -5.0 },
                    PlotPoint { x: 15.0, y: 15.0 },
                ],
            }],
        };

        let image = render_plot_for_bounds(
            &scene,
            PlotKind::Line,
            PlotBounds {
                x_min: 0.0,
                x_max: 10.0,
                y_min: 0.0,
                y_max: 10.0,
            },
        )
        .unwrap();
        let pixels = image.to_rgba8();
        let area = chart_geometry();
        let expected_color = Rgba(CHART_STROKE[0]);

        let mut has_line_pixel = false;
        for y in area.top..=area.bottom {
            for x in area.left..=area.right {
                if pixels.get_pixel(x as u32, y as u32) == &expected_color {
                    has_line_pixel = true;
                    break;
                }
            }
            if has_line_pixel {
                break;
            }
        }

        assert!(
            has_line_pixel,
            "crossing segment should render a visible chart pixel"
        );
    }
}
