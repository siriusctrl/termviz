use anyhow::{Result, anyhow};
use image::{DynamicImage, Rgba, RgbaImage};

use crate::plot::{PlotKind, model::PlotPoint, model::PlotScene};

const PLOT_CANVAS_WIDTH: u32 = 640;
const PLOT_CANVAS_HEIGHT: u32 = 360;
const LEFT_MARGIN: f64 = 52.0;
const RIGHT_MARGIN: f64 = 16.0;
const TOP_MARGIN: f64 = 16.0;
const BOTTOM_MARGIN: f64 = 24.0;
const MARK_RADIUS: i32 = 2;
const AXIS_COLOR: Rgba<u8> = Rgba([34, 34, 34, 255]);
const BACKGROUND_COLOR: Rgba<u8> = Rgba([255, 255, 255, 255]);
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
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
    width: f64,
    height: f64,
}

pub(crate) fn render_plot(scene: &PlotScene, kind: PlotKind) -> Result<DynamicImage> {
    let mut image = RgbaImage::from_pixel(PLOT_CANVAS_WIDTH, PLOT_CANVAS_HEIGHT, BACKGROUND_COLOR);

    if scene.series.is_empty() {
        return Ok(DynamicImage::ImageRgba8(image));
    }

    let bounds = scene
        .bounds()
        .ok_or_else(|| anyhow!("plot scene is empty"))?;
    let area = chart_geometry();
    draw_frame(&mut image, area);

    for (series_index, series) in scene.series.iter().enumerate() {
        let color = Rgba(CHART_STROKE[series_index % CHART_STROKE.len()]);
        let mut mapped = Vec::with_capacity(series.points.len());

        for point in &series.points {
            mapped.push(map_point(point, bounds, &area));
        }

        match kind {
            PlotKind::Line => {
                for pair in mapped.windows(2) {
                    let (x0, y0) = pair[0];
                    let (x1, y1) = pair[1];
                    draw_line(&mut image, x0, y0, x1, y1, color);
                }
                if let Some(&(x, y)) = mapped.first() {
                    draw_dot(&mut image, x, y, MARK_RADIUS, color);
                }
                if let Some(&(x, y)) = mapped.last() {
                    draw_dot(&mut image, x, y, MARK_RADIUS, color);
                }
            }
            PlotKind::Scatter => {
                for (x, y) in mapped {
                    draw_dot(&mut image, x, y, MARK_RADIUS, color);
                }
            }
        }
    }

    Ok(DynamicImage::ImageRgba8(image))
}

fn chart_geometry() -> PlotArea {
    let width = f64::from(PLOT_CANVAS_WIDTH) - LEFT_MARGIN - RIGHT_MARGIN;
    let height = f64::from(PLOT_CANVAS_HEIGHT) - TOP_MARGIN - BOTTOM_MARGIN;
    let left = LEFT_MARGIN.max(0.0) as u32;
    let right = (LEFT_MARGIN + width).round() as u32;
    let top = TOP_MARGIN.max(0.0) as u32;
    let bottom = (TOP_MARGIN + height).round() as u32;
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
    if area.right <= area.left || area.bottom <= area.top {
        return;
    }

    for x in area.left..=area.right {
        set_pixel(image, x, area.bottom, AXIS_COLOR);
    }
    for y in area.top..=area.bottom {
        set_pixel(image, area.left, y, AXIS_COLOR);
    }
    set_pixel(image, area.left, area.top, AXIS_COLOR);
    set_pixel(image, area.left, area.bottom, AXIS_COLOR);
}

fn map_point(
    point: &PlotPoint,
    bounds: crate::plot::model::PlotBounds,
    area: &PlotArea,
) -> (i32, i32) {
    let x_span = (bounds.x_max - bounds.x_min).abs().max(f64::EPSILON);
    let y_span = (bounds.y_max - bounds.y_min).abs().max(f64::EPSILON);
    let x_ratio = ((point.x - bounds.x_min) / x_span).clamp(0.0, 1.0);
    let y_ratio = ((point.y - bounds.y_min) / y_span).clamp(0.0, 1.0);
    let px = area.left as f64 + area.width * x_ratio;
    let py = (area.top as f64 + area.height) - (area.height * y_ratio);

    (
        px.round().clamp(area.left as f64, area.right as f64) as i32,
        py.round().clamp(area.top as f64, area.bottom as f64) as i32,
    )
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

fn set_pixel_if_in_bounds(image: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    if x < 0 || y < 0 {
        return;
    }
    let width = i32::try_from(image.width()).unwrap_or(i32::MAX);
    let height = i32::try_from(image.height()).unwrap_or(i32::MAX);
    if x < width && y < height {
        set_pixel(image, x as u32, y as u32, color);
    }
}

fn set_pixel(image: &mut RgbaImage, x: u32, y: u32, color: Rgba<u8>) {
    image.put_pixel(x, y, color);
}
