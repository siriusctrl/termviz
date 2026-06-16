use anyhow::{Context, Result};
use image::{DynamicImage, Rgba, RgbaImage};
#[cfg(test)]
use std::time::{Duration, Instant};

use crate::plot::{
    PlotKind,
    model::{PlotBounds, PlotScene},
};

use super::{
    display_list::{
        LineStyle, PlotCommand, PlotDisplayList, build_body_base_display_list,
        build_body_display_list, build_body_marks_display_list, build_display_list,
    },
    layout::{PlotDimensions, export_dimensions, interactive_text_scale},
    text::{TextMetrics, draw_text, draw_text_clipped},
    theme::{EXPORT_THEME, INTERACTIVE_THEME, PlotTheme},
};

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

pub(super) fn render_interactive_plot_body_for_size(
    scene: &PlotScene,
    kind: PlotKind,
    viewport: PlotBounds,
    width: u32,
    height: u32,
) -> Result<DynamicImage> {
    Ok(DynamicImage::ImageRgba8(
        render_interactive_plot_body_rgba_for_size(scene, kind, viewport, width, height)?,
    ))
}

pub(super) fn render_interactive_plot_body_rgba_for_size(
    scene: &PlotScene,
    kind: PlotKind,
    viewport: PlotBounds,
    width: u32,
    height: u32,
) -> Result<RgbaImage> {
    let dimensions = PlotDimensions::new(width, height);
    let text = TextMetrics::new(1);
    let list = build_body_display_list(scene, kind, viewport, dimensions, INTERACTIVE_THEME, text);
    Ok(render_display_list(&list))
}

pub(super) fn render_interactive_plot_body_base_rgba_for_size(
    scene: &PlotScene,
    viewport: PlotBounds,
    width: u32,
    height: u32,
) -> Result<RgbaImage> {
    let dimensions = PlotDimensions::new(width, height);
    let text = TextMetrics::new(1);
    let list = build_body_base_display_list(scene, viewport, dimensions, INTERACTIVE_THEME, text);
    Ok(render_display_list(&list))
}

pub(super) fn render_interactive_plot_body_marks_rgba_for_size(
    scene: &PlotScene,
    kind: PlotKind,
    viewport: PlotBounds,
    width: u32,
    height: u32,
) -> Result<RgbaImage> {
    let dimensions = PlotDimensions::new(width, height);
    let text = TextMetrics::new(1);
    let list =
        build_body_marks_display_list(scene, kind, viewport, dimensions, INTERACTIVE_THEME, text);
    Ok(render_display_list(&list))
}

#[cfg(test)]
pub(super) struct TimedPlotRaster {
    pub(super) image: RgbaImage,
    pub(super) display_list: Duration,
    pub(super) raster: Duration,
    pub(super) command_count: usize,
}

#[cfg(test)]
pub(super) fn render_interactive_plot_timed_for_size(
    scene: &PlotScene,
    kind: PlotKind,
    viewport: PlotBounds,
    width: u32,
    height: u32,
) -> Result<TimedPlotRaster> {
    let dimensions = PlotDimensions::new(width, height);
    let text = TextMetrics::new(interactive_text_scale(dimensions));
    let display_list_start = Instant::now();
    let list = build_display_list(scene, kind, viewport, dimensions, INTERACTIVE_THEME, text);
    let display_list = display_list_start.elapsed();
    let command_count = list.commands.len();

    let raster_start = Instant::now();
    let image = render_display_list(&list);
    let raster = raster_start.elapsed();

    Ok(TimedPlotRaster {
        image,
        display_list,
        raster,
        command_count,
    })
}

#[cfg(test)]
pub(super) fn render_interactive_plot_body_timed_for_size(
    scene: &PlotScene,
    kind: PlotKind,
    viewport: PlotBounds,
    width: u32,
    height: u32,
) -> Result<TimedPlotRaster> {
    let dimensions = PlotDimensions::new(width, height);
    let text = TextMetrics::new(1);
    let display_list_start = Instant::now();
    let list = build_body_display_list(scene, kind, viewport, dimensions, INTERACTIVE_THEME, text);
    let display_list = display_list_start.elapsed();
    let command_count = list.commands.len();

    let raster_start = Instant::now();
    let image = render_display_list(&list);
    let raster = raster_start.elapsed();

    Ok(TimedPlotRaster {
        image,
        display_list,
        raster,
        command_count,
    })
}

fn render_plot_with_theme(
    scene: &PlotScene,
    kind: PlotKind,
    viewport: PlotBounds,
    dimensions: PlotDimensions,
    theme: PlotTheme,
    text: TextMetrics,
) -> Result<DynamicImage> {
    let list = build_display_list(scene, kind, viewport, dimensions, theme, text);
    Ok(DynamicImage::ImageRgba8(render_display_list(&list)))
}

fn render_display_list(list: &PlotDisplayList) -> RgbaImage {
    let mut image = RgbaImage::from_pixel(
        list.dimensions.width,
        list.dimensions.height,
        list.background,
    );

    for command in &list.commands {
        match command {
            PlotCommand::Line {
                start,
                end,
                color,
                style,
            } => match style {
                LineStyle::Solid => draw_line(&mut image, start.x, start.y, end.x, end.y, *color),
                LineStyle::Dotted => {
                    draw_dotted_line(&mut image, start.x, start.y, end.x, end.y, *color);
                }
            },
            PlotCommand::Dot {
                center,
                radius,
                color,
            } => draw_dot(&mut image, center.x, center.y, *radius, *color),
            PlotCommand::Text {
                origin,
                content,
                color,
                max_width,
            } => match max_width {
                Some(max_width) => draw_text_clipped(
                    &mut image, list.text, origin.x, origin.y, content, *color, *max_width,
                ),
                None => draw_text(&mut image, list.text, origin.x, origin.y, content, *color),
            },
        }
    }

    image
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

fn draw_dotted_line(
    image: &mut RgbaImage,
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    color: Rgba<u8>,
) {
    if start_x == end_x {
        let min_y = start_y.min(end_y);
        let max_y = start_y.max(end_y);
        for y in min_y..=max_y {
            if y % 4 == 0 {
                set_pixel_if_in_bounds(image, start_x, y, color);
            }
        }
        return;
    }

    if start_y == end_y {
        let min_x = start_x.min(end_x);
        let max_x = start_x.max(end_x);
        for x in min_x..=max_x {
            if x % 4 == 0 {
                set_pixel_if_in_bounds(image, x, start_y, color);
            }
        }
        return;
    }

    draw_line(image, start_x, start_y, end_x, end_y, color);
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
        image.put_pixel(x as u32, y as u32, color);
    }
}
