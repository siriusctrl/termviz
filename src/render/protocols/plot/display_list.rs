use image::Rgba;

use crate::plot::{
    PlotKind,
    model::{PlotBounds, PlotPoint, PlotScene, PlotSeries},
};

use super::{
    layout::{PlotArea, PlotDimensions, PlotLayout, body_layout_for, layout_for, legend_label},
    text::TextMetrics,
    theme::PlotTheme,
};

const MARK_RADIUS: i32 = 2;
const DOWNSAMPLE_POINTS_PER_PIXEL: usize = 4;

#[derive(Debug, Clone)]
pub(super) struct PlotDisplayList {
    pub(super) dimensions: PlotDimensions,
    pub(super) background: Rgba<u8>,
    pub(super) text: TextMetrics,
    pub(super) commands: Vec<PlotCommand>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum PlotCommand {
    Line {
        start: ScreenPoint,
        end: ScreenPoint,
        color: Rgba<u8>,
        style: LineStyle,
    },
    Dot {
        center: ScreenPoint,
        radius: i32,
        color: Rgba<u8>,
    },
    Text {
        origin: ScreenPoint,
        content: String,
        color: Rgba<u8>,
        max_width: Option<i32>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LineStyle {
    Solid,
    Dotted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ScreenPoint {
    pub(super) x: i32,
    pub(super) y: i32,
}

pub(super) fn build_display_list(
    scene: &PlotScene,
    kind: PlotKind,
    viewport: PlotBounds,
    dimensions: PlotDimensions,
    theme: PlotTheme,
    text: TextMetrics,
) -> PlotDisplayList {
    let mut list = PlotDisplayList {
        dimensions,
        background: theme.background,
        text,
        commands: Vec::new(),
    };

    if scene.series.is_empty() {
        return list;
    }

    let bounds = viewport.normalized();
    let layout = layout_for(dimensions, scene, text);
    push_frame(&mut list, layout.area, theme.axis);
    push_grid_lines(&mut list, layout.area, theme.grid);
    push_axes(&mut list, layout, bounds, theme.axis, theme.text);
    push_axis_labels(&mut list, layout, theme.text);

    for (series_index, series) in scene.series.iter().enumerate() {
        let color = Rgba(theme.strokes[series_index % theme.strokes.len()]);
        match kind {
            PlotKind::Line => push_line_series(&mut list, series, bounds, &layout.area, color),
            PlotKind::Scatter => {
                push_scatter_series(&mut list, series, bounds, &layout.area, color)
            }
        }
    }

    push_title(&mut list, scene, layout, theme.title);
    push_legend(&mut list, scene, layout, theme);
    list
}

pub(super) fn build_body_display_list(
    scene: &PlotScene,
    kind: PlotKind,
    viewport: PlotBounds,
    dimensions: PlotDimensions,
    theme: PlotTheme,
    text: TextMetrics,
) -> PlotDisplayList {
    let mut list = PlotDisplayList {
        dimensions,
        background: theme.background,
        text,
        commands: Vec::new(),
    };

    if scene.series.is_empty() {
        return list;
    }

    let bounds = viewport.normalized();
    let layout = body_layout_for(dimensions, text);
    push_frame(&mut list, layout.area, theme.axis);
    push_grid_lines(&mut list, layout.area, theme.grid);

    for (series_index, series) in scene.series.iter().enumerate() {
        let color = Rgba(theme.strokes[series_index % theme.strokes.len()]);
        match kind {
            PlotKind::Line => push_line_series(&mut list, series, bounds, &layout.area, color),
            PlotKind::Scatter => {
                push_scatter_series(&mut list, series, bounds, &layout.area, color)
            }
        }
    }

    list
}

fn push_frame(list: &mut PlotDisplayList, area: PlotArea, color: Rgba<u8>) {
    push_line(
        list,
        area.left,
        area.top,
        area.right,
        area.top,
        color,
        LineStyle::Solid,
    );
    push_line(
        list,
        area.left,
        area.bottom,
        area.right,
        area.bottom,
        color,
        LineStyle::Solid,
    );
    push_line(
        list,
        area.left,
        area.top,
        area.left,
        area.bottom,
        color,
        LineStyle::Solid,
    );
    push_line(
        list,
        area.right,
        area.top,
        area.right,
        area.bottom,
        color,
        LineStyle::Solid,
    );
}

fn push_grid_lines(list: &mut PlotDisplayList, area: PlotArea, color: Rgba<u8>) {
    let x_steps = 4;
    let y_steps = 4;
    for step in 1..x_steps {
        let x = area.left + ((area.width * (step as f64) / x_steps as f64) as i32);
        push_line(
            list,
            x,
            area.top + 1,
            x,
            area.bottom - 1,
            color,
            LineStyle::Dotted,
        );
    }

    for step in 1..y_steps {
        let y = area.top + ((area.height * (step as f64) / y_steps as f64) as i32);
        push_line(
            list,
            area.left + 1,
            y,
            area.right - 1,
            y,
            color,
            LineStyle::Dotted,
        );
    }
}

fn push_axes(
    list: &mut PlotDisplayList,
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
        push_dot(
            list,
            x.min(area.right),
            area.bottom.saturating_sub(2),
            0,
            axis_color,
        );
        push_text(
            list,
            label_x,
            area.bottom + 8 * layout.text.scale(),
            label,
            text_color,
            None,
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
        push_dot(list, area.left + 1, y.min(area.bottom), 0, axis_color);
        push_text(
            list,
            label_x,
            y.saturating_sub(layout.text.glyph_height() / 2),
            label,
            text_color,
            None,
        );
    }
}

fn push_axis_labels(list: &mut PlotDisplayList, layout: PlotLayout, color: Rgba<u8>) {
    let area = layout.area;
    push_text(
        list,
        area.left + (area.width / 2.0).round() as i32,
        area.bottom + 28 * layout.text.scale(),
        "x".to_owned(),
        color,
        None,
    );
    push_text(
        list,
        area.left.saturating_sub(18 * layout.text.scale()),
        area.top
            .saturating_sub(layout.text.glyph_height() + 5 * layout.text.scale()),
        "y".to_owned(),
        color,
        None,
    );
}

fn push_line_series(
    list: &mut PlotDisplayList,
    series: &PlotSeries,
    bounds: PlotBounds,
    area: &PlotArea,
    color: Rgba<u8>,
) {
    if should_downsample(series, area) {
        push_downsampled_line_series(list, series, bounds, area, color);
    } else {
        push_full_line_series(list, series, bounds, area, color);
    }

    let visible: Vec<_> = series
        .points
        .iter()
        .filter(|point| is_point_in_bounds(point, &bounds))
        .map(|point| map_point(point, bounds, area))
        .collect();
    if let Some(point) = visible.first() {
        push_dot(list, point.x, point.y, MARK_RADIUS, color);
    }
    if let Some(point) = visible.last() {
        push_dot(list, point.x, point.y, MARK_RADIUS, color);
    }
}

fn push_full_line_series(
    list: &mut PlotDisplayList,
    series: &PlotSeries,
    bounds: PlotBounds,
    area: &PlotArea,
    color: Rgba<u8>,
) {
    for pair in series.points.windows(2) {
        let Some((start, end)) = clip_to_bounds(&pair[0], &pair[1], bounds) else {
            continue;
        };
        let start = map_point(&start, bounds, area);
        let end = map_point(&end, bounds, area);
        push_line(
            list,
            start.x,
            start.y,
            end.x,
            end.y,
            color,
            LineStyle::Solid,
        );
    }
}

fn push_downsampled_line_series(
    list: &mut PlotDisplayList,
    series: &PlotSeries,
    bounds: PlotBounds,
    area: &PlotArea,
    color: Rgba<u8>,
) {
    let width = usize::try_from((area.right - area.left + 1).max(1)).unwrap_or(1);
    let mut buckets = vec![PixelBucket::default(); width];

    for pair in series.points.windows(2) {
        let Some((start, end)) = clip_to_bounds(&pair[0], &pair[1], bounds) else {
            continue;
        };
        add_bucket_point(&mut buckets, area, map_point(&start, bounds, area));
        add_bucket_point(&mut buckets, area, map_point(&end, bounds, area));
    }

    let mut previous: Option<ScreenPoint> = None;
    for (index, bucket) in buckets.into_iter().enumerate() {
        if !bucket.seen {
            continue;
        }
        let x = area.left + i32::try_from(index).unwrap_or(i32::MAX);
        if let Some(previous) = previous {
            push_line(
                list,
                previous.x,
                previous.y,
                x,
                bucket.first_y,
                color,
                LineStyle::Solid,
            );
        }
        if bucket.min_y != bucket.max_y {
            push_line(
                list,
                x,
                bucket.min_y,
                x,
                bucket.max_y,
                color,
                LineStyle::Solid,
            );
        } else {
            push_dot(list, x, bucket.min_y, 0, color);
        }
        previous = Some(ScreenPoint {
            x,
            y: bucket.last_y,
        });
    }
}

fn push_scatter_series(
    list: &mut PlotDisplayList,
    series: &PlotSeries,
    bounds: PlotBounds,
    area: &PlotArea,
    color: Rgba<u8>,
) {
    let mut previous: Option<ScreenPoint> = None;
    for point in series
        .points
        .iter()
        .filter(|point| is_point_in_bounds(point, &bounds))
        .map(|point| map_point(point, bounds, area))
    {
        if previous == Some(point) {
            continue;
        }
        push_dot(list, point.x, point.y, MARK_RADIUS, color);
        previous = Some(point);
    }
}

fn push_title(list: &mut PlotDisplayList, scene: &PlotScene, layout: PlotLayout, color: Rgba<u8>) {
    let title = scene.title.as_deref().unwrap_or("plot");
    let summary = format!(
        "{}  points: {}  series: {}",
        title,
        scene.total_points(),
        scene.series.len(),
    );
    let max_width = layout.legend.left.saturating_sub(layout.header_padding * 2);
    push_text(
        list,
        layout.header_padding,
        layout.title_y,
        summary,
        color,
        Some(max_width),
    );
}

fn push_legend(
    list: &mut PlotDisplayList,
    scene: &PlotScene,
    layout: PlotLayout,
    theme: PlotTheme,
) {
    for (index, series) in scene.series.iter().take(layout.legend_max_rows).enumerate() {
        let color = Rgba(theme.strokes[index % theme.strokes.len()]);
        let row_y = layout.legend_top + index as i32 * layout.legend_row_height;
        let swatch_y = row_y + layout.text.glyph_height() / 2;
        push_line(
            list,
            layout.legend.left,
            swatch_y,
            layout.legend.left + layout.legend_swatch_width - 1,
            swatch_y,
            color,
            LineStyle::Solid,
        );
        push_dot(
            list,
            layout.legend.left + layout.legend_swatch_width / 2,
            swatch_y,
            1,
            color,
        );

        let label = legend_label(index, series);
        push_text(
            list,
            layout.legend.left + layout.legend_swatch_width + layout.legend_text_gap,
            row_y,
            label,
            color,
            Some(
                layout
                    .legend
                    .width
                    .saturating_sub(layout.legend_swatch_width + layout.legend_text_gap),
            ),
        );
    }
}

fn push_line(
    list: &mut PlotDisplayList,
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    color: Rgba<u8>,
    style: LineStyle,
) {
    list.commands.push(PlotCommand::Line {
        start: ScreenPoint {
            x: start_x,
            y: start_y,
        },
        end: ScreenPoint { x: end_x, y: end_y },
        color,
        style,
    });
}

fn push_dot(list: &mut PlotDisplayList, x: i32, y: i32, radius: i32, color: Rgba<u8>) {
    list.commands.push(PlotCommand::Dot {
        center: ScreenPoint { x, y },
        radius,
        color,
    });
}

fn push_text(
    list: &mut PlotDisplayList,
    x: i32,
    y: i32,
    content: String,
    color: Rgba<u8>,
    max_width: Option<i32>,
) {
    list.commands.push(PlotCommand::Text {
        origin: ScreenPoint { x, y },
        content,
        color,
        max_width,
    });
}

fn should_downsample(series: &PlotSeries, area: &PlotArea) -> bool {
    let width = usize::try_from((area.right - area.left + 1).max(1)).unwrap_or(1);
    series.points.len() > width.saturating_mul(DOWNSAMPLE_POINTS_PER_PIXEL)
        && is_sorted_by_x(&series.points)
}

fn is_sorted_by_x(points: &[PlotPoint]) -> bool {
    points.windows(2).all(|pair| pair[0].x <= pair[1].x)
}

fn add_bucket_point(buckets: &mut [PixelBucket], area: &PlotArea, point: ScreenPoint) {
    let index =
        usize::try_from((point.x - area.left).clamp(0, area.right - area.left)).unwrap_or(0);
    let bucket = &mut buckets[index];
    if bucket.seen {
        bucket.min_y = bucket.min_y.min(point.y);
        bucket.max_y = bucket.max_y.max(point.y);
        bucket.last_y = point.y;
    } else {
        *bucket = PixelBucket {
            seen: true,
            min_y: point.y,
            max_y: point.y,
            first_y: point.y,
            last_y: point.y,
        };
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PixelBucket {
    seen: bool,
    min_y: i32,
    max_y: i32,
    first_y: i32,
    last_y: i32,
}

fn map_point(point: &PlotPoint, bounds: PlotBounds, area: &PlotArea) -> ScreenPoint {
    let x_span = (bounds.x_max - bounds.x_min).abs().max(f64::EPSILON);
    let y_span = (bounds.y_max - bounds.y_min).abs().max(f64::EPSILON);
    let x_ratio = ((point.x - bounds.x_min) / x_span).clamp(0.0, 1.0);
    let y_ratio = ((point.y - bounds.y_min) / y_span).clamp(0.0, 1.0);

    let px = f64::from(area.left) + area.width * x_ratio;
    let py = (f64::from(area.top) + area.height) - (area.height * y_ratio);

    ScreenPoint {
        x: px
            .round()
            .clamp(f64::from(area.left), f64::from(area.right)) as i32,
        y: py
            .round()
            .clamp(f64::from(area.top), f64::from(area.bottom)) as i32,
    }
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
    use crate::render::protocols::plot::{
        layout::{export_dimensions, layout_for},
        text::TextMetrics,
        theme::{EXPORT_THEME, INTERACTIVE_THEME},
    };

    #[test]
    fn dense_sorted_line_series_downsamples_by_screen_width() {
        let scene = PlotScene {
            title: Some("dense".to_owned()),
            series: vec![PlotSeries {
                name: "svc".to_owned(),
                points: (0..10_000)
                    .map(|index| PlotPoint {
                        x: index as f64,
                        y: (index % 200) as f64,
                    })
                    .collect(),
            }],
        };
        let bounds = scene.bounds().unwrap().normalized();

        let list = build_display_list(
            &scene,
            PlotKind::Line,
            bounds,
            export_dimensions(),
            EXPORT_THEME,
            TextMetrics::new(1),
        );
        let series_line_count = list
            .commands
            .iter()
            .filter(|command| {
                matches!(
                    command,
                    PlotCommand::Line {
                        color,
                        style: LineStyle::Solid,
                        ..
                    } if *color == Rgba(EXPORT_THEME.strokes[0])
                )
            })
            .count();

        assert!(
            series_line_count < 2_000,
            "dense line should emit pixel-bucket commands, got {series_line_count}"
        );
    }

    #[test]
    fn crossing_line_segment_is_clipped_to_plot_area() {
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
        let viewport = PlotBounds {
            x_min: 0.0,
            x_max: 10.0,
            y_min: 0.0,
            y_max: 10.0,
        };
        let dimensions = export_dimensions();
        let text = TextMetrics::new(1);
        let layout = layout_for(dimensions, &scene, text);

        let list = build_display_list(
            &scene,
            PlotKind::Line,
            viewport,
            dimensions,
            EXPORT_THEME,
            text,
        );
        let series_color = Rgba(EXPORT_THEME.strokes[0]);
        let chart_lines = list
            .commands
            .iter()
            .filter_map(|command| match command {
                PlotCommand::Line {
                    start,
                    end,
                    color,
                    style: LineStyle::Solid,
                } if *color == series_color
                    && point_in_area(*start, layout.area)
                    && point_in_area(*end, layout.area) =>
                {
                    Some((*start, *end))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(chart_lines.len(), 1);
        assert_eq!(
            chart_lines[0],
            (
                ScreenPoint {
                    x: layout.area.left,
                    y: layout.area.bottom,
                },
                ScreenPoint {
                    x: layout.area.right,
                    y: layout.area.top,
                },
            )
        );
    }

    #[test]
    fn display_list_keeps_interactive_theme_background() {
        let scene = PlotScene {
            title: None,
            series: vec![PlotSeries {
                name: String::new(),
                points: vec![PlotPoint { x: 0.0, y: 0.0 }],
            }],
        };

        let list = build_display_list(
            &scene,
            PlotKind::Scatter,
            scene.bounds().unwrap(),
            export_dimensions(),
            INTERACTIVE_THEME,
            TextMetrics::new(1),
        );

        assert_eq!(list.background, INTERACTIVE_THEME.background);
    }

    fn point_in_area(point: ScreenPoint, area: PlotArea) -> bool {
        (area.left..=area.right).contains(&point.x) && (area.top..=area.bottom).contains(&point.y)
    }
}
