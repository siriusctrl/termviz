use anyhow::{Context, Result};
use image::{DynamicImage, Rgba, RgbaImage};

use crate::plot::{
    PlotKind,
    model::{PlotBounds, PlotPoint, PlotScene},
};

use super::{
    layout::{
        PlotArea, PlotDimensions, PlotLayout, export_dimensions, interactive_text_scale,
        layout_for, legend_label,
    },
    text::{TextMetrics, draw_text, draw_text_clipped},
    theme::{EXPORT_THEME, INTERACTIVE_THEME, PlotTheme},
};

const MARK_RADIUS: i32 = 2;

pub(super) fn render_export_plot(scene: &PlotScene, kind: PlotKind) -> Result<DynamicImage> {
    let bounds = scene.bounds().context("plot scene is empty")?.normalized();
    render_export_plot_for_bounds(scene, kind, bounds)
}

pub(super) fn render_export_plot_for_bounds(
    scene: &PlotScene,
    kind: PlotKind,
    viewport: PlotBounds,
) -> Result<DynamicImage> {
    render_plot_with_theme(
        scene,
        kind,
        viewport,
        export_dimensions(),
        EXPORT_THEME,
        TextMetrics::new(1),
    )
}

pub(super) fn render_interactive_plot_for_size(
    scene: &PlotScene,
    kind: PlotKind,
    viewport: PlotBounds,
    width: u32,
    height: u32,
) -> Result<DynamicImage> {
    let dimensions = PlotDimensions::new(width, height);
    let text = TextMetrics::new(interactive_text_scale(dimensions));
    render_plot_with_theme(scene, kind, viewport, dimensions, INTERACTIVE_THEME, text)
}

fn render_plot_with_theme(
    scene: &PlotScene,
    kind: PlotKind,
    viewport: PlotBounds,
    dimensions: PlotDimensions,
    theme: PlotTheme,
    text: TextMetrics,
) -> Result<DynamicImage> {
    let mut image = RgbaImage::from_pixel(dimensions.width, dimensions.height, theme.background);

    if scene.series.is_empty() {
        return Ok(DynamicImage::ImageRgba8(image));
    }

    let bounds = viewport.normalized();
    let layout = layout_for(dimensions, scene, text);
    draw_frame(&mut image, layout.area, theme.axis);
    draw_grid_lines(&mut image, layout.area, theme.grid);
    draw_axes(&mut image, layout, bounds, theme.axis, theme.text);
    draw_axis_labels(&mut image, layout, theme.text);

    for (series_index, series) in scene.series.iter().enumerate() {
        let color = Rgba(theme.strokes[series_index % theme.strokes.len()]);
        match kind {
            PlotKind::Line => {
                for pair in series.points.windows(2) {
                    let Some((start, end)) = clip_to_bounds(&pair[0], &pair[1], bounds) else {
                        continue;
                    };
                    let (x0, y0) = map_point(&start, bounds, &layout.area);
                    let (x1, y1) = map_point(&end, bounds, &layout.area);
                    draw_line(&mut image, x0, y0, x1, y1, color);
                }

                let visible: Vec<_> = series
                    .points
                    .iter()
                    .filter(|point| is_point_in_bounds(point, &bounds))
                    .map(|point| map_point(point, bounds, &layout.area))
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
                    .map(|point| map_point(point, bounds, &layout.area))
                    .collect();
                for (x, y) in mapped {
                    draw_dot(&mut image, x, y, MARK_RADIUS, color);
                }
            }
        }
    }

    draw_title(&mut image, scene, layout, theme.title);
    draw_legend(&mut image, scene, layout, theme);
    Ok(DynamicImage::ImageRgba8(image))
}

fn draw_frame(image: &mut RgbaImage, area: PlotArea, color: Rgba<u8>) {
    for x in area.left..=area.right {
        set_pixel_if_in_bounds(image, x, area.top, color);
        set_pixel_if_in_bounds(image, x, area.bottom, color);
    }

    for y in area.top..=area.bottom {
        set_pixel_if_in_bounds(image, area.left, y, color);
        set_pixel_if_in_bounds(image, area.right, y, color);
    }
}

fn draw_grid_lines(image: &mut RgbaImage, area: PlotArea, color: Rgba<u8>) {
    let x_steps = 4;
    let y_steps = 4;
    for step in 1..x_steps {
        let x = area.left + ((area.width * (step as f64) / x_steps as f64) as i32);
        for y in area.top + 1..area.bottom {
            if y % 4 == 0 {
                set_pixel_if_in_bounds(image, x, y, color);
            }
        }
    }

    for step in 1..y_steps {
        let y = area.top + ((area.height * (step as f64) / y_steps as f64) as i32);
        for x in area.left + 1..area.right {
            if x % 4 == 0 {
                set_pixel_if_in_bounds(image, x, y, color);
            }
        }
    }
}

fn draw_axes(
    image: &mut RgbaImage,
    layout: PlotLayout,
    bounds: PlotBounds,
    axis_color: Rgba<u8>,
    text_color: Rgba<u8>,
) {
    let area = layout.area;
    let y_span = (bounds.y_max - bounds.y_min).abs().max(f64::EPSILON);
    let x_span = (bounds.x_max - bounds.x_min).abs().max(f64::EPSILON);

    let x_tick_count = 5;
    for step in 0..x_tick_count {
        let ratio = step as f64 / (x_tick_count - 1) as f64;
        let x = (area.left as f64 + area.width * ratio).round() as i32;
        let value = bounds.x_min + x_span * ratio;
        let label = format_axis_label(value);
        let label_width = layout.text.width(&label);
        let label_x = (x - label_width / 2)
            .max(area.left)
            .min(area.right.saturating_sub(label_width));
        set_pixel_if_in_bounds(
            image,
            x.min(area.right),
            area.bottom.saturating_sub(2),
            axis_color,
        );
        draw_text(
            image,
            layout.text,
            label_x,
            area.bottom + 8 * layout.text.scale(),
            &label,
            text_color,
        );
    }

    let y_tick_count = 5;
    for step in 0..y_tick_count {
        let ratio = step as f64 / (y_tick_count - 1) as f64;
        let y = (area.top as f64 + area.height * ratio).round() as i32;
        let value = bounds.y_max - y_span * ratio;
        let label = format_axis_label(value);
        let label_width = layout.text.width(&label);
        let label_x = area
            .left
            .saturating_sub(layout.y_tick_gap)
            .saturating_sub(label_width)
            .max(2);
        set_pixel_if_in_bounds(image, area.left + 1, y.min(area.bottom), axis_color);
        draw_text(
            image,
            layout.text,
            label_x,
            y.saturating_sub(layout.text.glyph_height() / 2),
            &label,
            text_color,
        );
    }
}

fn draw_axis_labels(image: &mut RgbaImage, layout: PlotLayout, color: Rgba<u8>) {
    let area = layout.area;
    draw_text(
        image,
        layout.text,
        area.left + (area.width / 2.0).round() as i32,
        area.bottom + 28 * layout.text.scale(),
        "x",
        color,
    );
    draw_text(
        image,
        layout.text,
        area.left.saturating_sub(18 * layout.text.scale()),
        area.top
            .saturating_sub(layout.text.glyph_height() + 5 * layout.text.scale()),
        "y",
        color,
    );
}

fn draw_title(image: &mut RgbaImage, scene: &PlotScene, layout: PlotLayout, color: Rgba<u8>) {
    let title = scene.title.as_deref().unwrap_or("plot");
    let summary = format!(
        "{}  points: {}  series: {}",
        title,
        scene.total_points(),
        scene.series.len(),
    );
    let max_width = layout.legend.left.saturating_sub(layout.header_padding * 2);
    draw_text_clipped(
        image,
        layout.text,
        layout.header_padding,
        layout.title_y,
        &summary,
        color,
        max_width,
    );
}

fn draw_legend(image: &mut RgbaImage, scene: &PlotScene, layout: PlotLayout, theme: PlotTheme) {
    for (index, series) in scene.series.iter().take(layout.legend_max_rows).enumerate() {
        let color = Rgba(theme.strokes[index % theme.strokes.len()]);
        let row_y = layout.legend_top + index as i32 * layout.legend_row_height;
        let swatch_y = row_y + layout.text.glyph_height() / 2;
        for pixel in 0..layout.legend_swatch_width {
            set_pixel_if_in_bounds(image, layout.legend.left + pixel, swatch_y, color);
        }
        draw_dot(
            image,
            layout.legend.left + layout.legend_swatch_width / 2,
            swatch_y,
            1,
            color,
        );

        let label = legend_label(index, series);
        draw_text_clipped(
            image,
            layout.text,
            layout.legend.left + layout.legend_swatch_width + layout.legend_text_gap,
            row_y,
            &label,
            color,
            layout
                .legend
                .width
                .saturating_sub(layout.legend_swatch_width + layout.legend_text_gap),
        );
    }
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

fn format_axis_label(value: f64) -> String {
    if value.abs() >= 10_000.0 {
        format!("{value:.1e}")
    } else if value.abs() >= 100.0 {
        format!("{value:.2}")
    } else {
        format!("{value:.3}")
    }
}
