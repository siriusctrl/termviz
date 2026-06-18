use crossterm::style::Color;
use unicode_width::UnicodeWidthChar;

use crate::{
    plot::model::{PlotBounds, PlotScene},
    render::Protocol,
    tui::{
        ChromeLine, ChromeRole, ChromeSegment, DynamicPlotChrome, PlotAxisLabel, PlotImageBody,
        PlotLegendItem, PlotProtocolChrome, PlotProtocolFrame, StaticPlotChrome, TerminalSize,
    },
};

use super::hover::{PlotHover, format_hover_value};
use super::state::PlotViewState;

pub(super) const CELL_PIXEL_WIDTH: u32 = 8;
pub(super) const CELL_PIXEL_HEIGHT: u32 = 16;
pub(super) const MAX_INTERACTIVE_PLOT_PIXELS: u64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PlotProtocolLayout {
    pub(super) image_col: u16,
    pub(super) image_row: u16,
    pub(super) image_cols: u16,
    pub(super) image_rows: u16,
    pub(super) x_axis_row: u16,
    pub(super) readout_row: u16,
}

pub(super) struct PlotProtocolChromeLines {
    pub(super) readout: ChromeLine,
    pub(super) status: ChromeLine,
}

pub(super) fn plot_protocol_layout(size: TerminalSize) -> PlotProtocolLayout {
    let header_rows = if size.height >= 12 {
        3
    } else if size.height >= 8 {
        2
    } else {
        1
    };
    let x_axis_rows = if size.height >= 8 { 1 } else { 0 };
    let readout_rows = if size.height >= 10 { 1 } else { 0 };
    let status_rows = 1;
    let y_axis_cols = if size.width >= 56 { 9 } else { 0 };
    let reserved_rows = header_rows + x_axis_rows + readout_rows + status_rows;
    let image_rows = size.height.saturating_sub(reserved_rows).max(1);
    let image_cols = size.width.saturating_sub(y_axis_cols).max(1);
    let image_row = header_rows.min(size.height.saturating_sub(1));
    let x_axis_row = image_row
        .saturating_add(image_rows)
        .min(size.height.saturating_sub(2));
    let readout_row = x_axis_row
        .saturating_add(x_axis_rows)
        .min(size.height.saturating_sub(1));

    PlotProtocolLayout {
        image_col: y_axis_cols,
        image_row,
        image_cols,
        image_rows,
        x_axis_row,
        readout_row,
    }
}

pub(super) fn plot_protocol_chrome<'a>(
    payload: &'a str,
    scene: &PlotScene,
    state: &PlotViewState,
    protocol: Protocol,
    size: TerminalSize,
    lines: PlotProtocolChromeLines,
    repaint_static_chrome: bool,
) -> PlotProtocolFrame<'a> {
    let layout = plot_protocol_layout(size);
    PlotProtocolFrame {
        payload,
        body: PlotImageBody {
            col: layout.image_col,
            row: layout.image_row,
            cols: layout.image_cols,
            rows: layout.image_rows,
        },
        chrome: PlotProtocolChrome {
            static_layer: StaticPlotChrome {
                repaint: repaint_static_chrome,
                x_axis_row: layout.x_axis_row,
                legend: plot_legend(scene, size.width),
            },
            dynamic_layer: DynamicPlotChrome {
                header: plot_header(scene, state, protocol),
                y_labels: plot_y_labels(state.visible, layout),
                x_labels: plot_x_labels(state.visible, layout, size.width),
                readout_row: layout.readout_row,
                readout: lines.readout,
                status: lines.status,
            },
        },
    }
}

fn plot_header(scene: &PlotScene, state: &PlotViewState, protocol: Protocol) -> ChromeLine {
    let title = scene.title.as_deref().unwrap_or("plot");
    let mode = if state.fit_mode { "fit" } else { "pan/zoom" };
    ChromeLine::new(vec![
        ChromeSegment::new(title, ChromeRole::Title),
        ChromeSegment::new(protocol_label(protocol), ChromeRole::Meta),
        ChromeSegment::new(mode, ChromeRole::State),
        ChromeSegment::new(format!("{} series", scene.series.len()), ChromeRole::Meta),
        ChromeSegment::new(format!("{} pts", scene.total_points()), ChromeRole::Meta),
    ])
}

fn plot_legend(scene: &PlotScene, width: u16) -> Vec<PlotLegendItem> {
    let mut remaining = usize::from(width.saturating_sub(2));
    let mut items = Vec::new();
    for (index, series) in scene.series.iter().enumerate() {
        let label = if series.name.is_empty() {
            format!("series {} ({})", index + 1, series.points.len())
        } else {
            format!("{} ({})", series.name, series.points.len())
        };
        let marker = "━━";
        let item_width = marker.chars().count() + 1 + label.chars().count() + 2;
        if item_width > remaining && !items.is_empty() {
            break;
        }
        remaining = remaining.saturating_sub(item_width);
        items.push(PlotLegendItem {
            marker: marker.to_owned(),
            label,
            color: terminal_series_color(index),
        });
    }
    items
}

fn plot_y_labels(bounds: PlotBounds, layout: PlotProtocolLayout) -> Vec<PlotAxisLabel> {
    if layout.image_col == 0 {
        return Vec::new();
    }
    let span = (bounds.y_max - bounds.y_min).abs().max(f64::EPSILON);
    let mut labels = Vec::new();
    for step in 0..5 {
        let ratio = step as f64 / 4.0;
        let value = bounds.y_max - span * ratio;
        let row = layout.image_row
            + ((f64::from(layout.image_rows.saturating_sub(1)) * ratio).round() as u16);
        labels.push(PlotAxisLabel {
            col: 0,
            row,
            text: format!("{:>8}", format_axis_label(value)),
        });
    }
    labels
}

fn plot_x_labels(
    bounds: PlotBounds,
    layout: PlotProtocolLayout,
    terminal_width: u16,
) -> Vec<PlotAxisLabel> {
    let span = (bounds.x_max - bounds.x_min).abs().max(f64::EPSILON);
    let mut labels = Vec::new();
    for step in 0..5 {
        let ratio = step as f64 / 4.0;
        let value = bounds.x_min + span * ratio;
        let text = format_axis_label(value);
        let offset = ((f64::from(layout.image_cols.saturating_sub(1)) * ratio).round() as u16)
            .saturating_sub((text.chars().count() / 2) as u16);
        let col = layout
            .image_col
            .saturating_add(offset)
            .min(terminal_width.saturating_sub(text.chars().count() as u16));
        labels.push(PlotAxisLabel {
            col,
            row: layout.x_axis_row,
            text,
        });
    }
    labels
}

fn terminal_series_color(index: usize) -> Color {
    const COLORS: [Color; 6] = [
        Color::Rgb {
            r: 86,
            g: 199,
            b: 217,
        },
        Color::Rgb {
            r: 242,
            g: 166,
            b: 90,
        },
        Color::Rgb {
            r: 136,
            g: 199,
            b: 121,
        },
        Color::Rgb {
            r: 216,
            g: 140,
            b: 190,
        },
        Color::Rgb {
            r: 143,
            g: 167,
            b: 255,
        },
        Color::Rgb {
            r: 233,
            g: 214,
            b: 107,
        },
    ];
    COLORS[index % COLORS.len()]
}

fn format_axis_label(value: f64) -> String {
    if value.abs() >= 10_000.0 {
        format!("{value:.1e}")
    } else if value.fract().abs() < 0.001 {
        format!("{value:.0}")
    } else if value.abs() >= 100.0 {
        format!("{value:.1}")
    } else if value.abs() >= 10.0 {
        format!("{value:.2}")
    } else {
        format!("{value:.3}")
    }
}

pub(super) fn pixel_protocol_target_size(protocol: Protocol, cols: u32, rows: u32) -> (u32, u32) {
    let width = cols.max(1).saturating_mul(CELL_PIXEL_WIDTH);
    let height = rows.max(1).saturating_mul(CELL_PIXEL_HEIGHT);
    match protocol {
        Protocol::Kitty => cap_pixel_target(width, height),
        Protocol::Blocks | Protocol::Auto => {
            unreachable!("pixel target is only used for pixel protocols")
        }
    }
}

fn cap_pixel_target(width: u32, height: u32) -> (u32, u32) {
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if pixels <= MAX_INTERACTIVE_PLOT_PIXELS {
        return (width.max(1), height.max(1));
    }

    let scale = (MAX_INTERACTIVE_PLOT_PIXELS as f64 / pixels as f64).sqrt();
    let mut capped_width = ((width as f64 * scale).floor()).max(1.0) as u32;
    let mut capped_height = ((height as f64 * scale).floor()).max(1.0) as u32;
    while u64::from(capped_width).saturating_mul(u64::from(capped_height))
        > MAX_INTERACTIVE_PLOT_PIXELS
    {
        if capped_width >= capped_height && capped_width > 1 {
            capped_width -= 1;
        } else if capped_height > 1 {
            capped_height -= 1;
        } else {
            break;
        }
    }
    (capped_width, capped_height)
}

pub(super) fn status_line_text(size: TerminalSize, show_overlay: bool) -> String {
    let status = status_line_chrome(show_overlay)
        .segments
        .into_iter()
        .map(|segment| segment.text)
        .collect::<Vec<_>>()
        .join(" · ");
    trim_to_width(&status, usize::from(size.width.max(1)))
}

pub(super) fn status_line_chrome(show_overlay: bool) -> ChromeLine {
    let overlay_hint = if show_overlay { "chart m" } else { "info m" };
    ChromeLine::new(vec![
        ChromeSegment::new("pan arrows", ChromeRole::Action),
        ChromeSegment::new("zoom +/-", ChromeRole::Action),
        ChromeSegment::new("fit 0", ChromeRole::Action),
        ChromeSegment::new(overlay_hint, ChromeRole::State),
        ChromeSegment::new("quit q", ChromeRole::Action),
    ])
}

pub(super) fn readout_line_chrome(hover: Option<&PlotHover>) -> ChromeLine {
    let Some(hover) = hover else {
        return ChromeLine::default();
    };
    let mut segments = vec![ChromeSegment::new(
        format!("x {}", format_hover_value(hover.x)),
        ChromeRole::State,
    )];
    if hover.samples.is_empty() {
        segments.push(ChromeSegment::new("no visible points", ChromeRole::Muted));
    } else {
        segments.extend(hover.samples.iter().map(|sample| {
            ChromeSegment::new(
                format!(
                    "{} ({}, {})",
                    sample.label,
                    format_hover_value(sample.point.x),
                    format_hover_value(sample.point.y)
                ),
                ChromeRole::Meta,
            )
        }));
        if hover.hidden_samples > 0 {
            segments.push(ChromeSegment::new(
                format!("+{} more", hover.hidden_samples),
                ChromeRole::Muted,
            ));
        }
    }
    ChromeLine::new(segments)
}

fn protocol_label(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Auto => "auto",
        Protocol::Kitty => "kitty",
        Protocol::Blocks => "blocks",
    }
}

pub(super) fn render_plot_overlay(scene: &PlotScene) -> String {
    let mut output = String::new();
    let points = scene.total_points();
    let bounds = scene.bounds();
    output.push_str(&format!("points: {points}\n"));
    output.push_str(&format!("series: {}\n", scene.series.len()));

    if let Some(bounds) = bounds {
        output.push_str(&format!("x: [{:.3}, {:.3}]\n", bounds.x_min, bounds.x_max));
        output.push_str(&format!("y: [{:.3}, {:.3}]\n", bounds.y_min, bounds.y_max));
    } else {
        output.push_str("bounds: unavailable\n");
    }

    output.push_str("controls: arrows pan, +/- zoom, 0 fit, m compact, q quit");
    output
}

fn trim_to_width(text: &str, width: usize) -> String {
    let mut output = String::new();
    let mut used = 0usize;

    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(1).max(1);
        if used + ch_width > width {
            break;
        }
        used += ch_width;
        output.push(ch);
    }

    if used < width {
        output.push_str(&" ".repeat(width - used));
    }

    output
}
