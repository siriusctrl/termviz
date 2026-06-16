use std::borrow::Cow;

use anyhow::{Context, Result, bail};
use crossterm::{
    event::{Event, KeyCode, KeyEventKind},
    style::Color,
};
use unicode_width::UnicodeWidthChar;

use crate::{
    input::InputSource,
    plot::model::{PlotBounds, PlotScene},
    plot::{PlotKind, stream, table},
    profile::{ContentKind, InputProfile},
    render::{
        Protocol,
        protocols::{ProtocolRenderContext, blocks, render_plot_rgba_with_fallback},
    },
    tui::{PlotAxisLabel, PlotLegendItem, PlotProtocolFrame, TerminalSession, TerminalSize},
};

const PAN_STEP: f64 = 0.15;
const ZOOM_STEP: f64 = 1.2;
const MIN_SPAN: f64 = 1e-6;
const CELL_PIXEL_WIDTH: u32 = 8;
const CELL_PIXEL_HEIGHT: u32 = 16;
const MAX_PENDING_EVENTS_PER_FRAME: usize = 64;
const MAX_INTERACTIVE_PLOT_PIXELS: u64 = 1_000_000;

pub(crate) fn run(
    source: InputSource,
    profile: InputProfile,
    protocol: Protocol,
    request: PlotRequest,
) -> Result<()> {
    let scene = load_scene(
        &source,
        &profile,
        request.x.as_deref(),
        request.y.as_deref(),
        request.group.as_deref(),
    )?;
    let bounds = scene.bounds().context("plot scene is empty")?.normalized();
    let mut state = PlotViewState::new(bounds);
    let protocol = resolve_plot_protocol(protocol);

    let mut session = TerminalSession::start()?;
    let mut size = session.size().context("reading initial terminal size")?;
    let mut show_overlay = false;
    let mut dirty = true;
    let mut frame_cache = PlotFrameCache::default();
    let mut last_protocol_chrome_size = None;

    loop {
        if dirty {
            let outcome =
                drain_pending_plot_events(&mut session, &mut size, &mut state, &mut show_overlay)?;
            if outcome.quit {
                break;
            }
            if outcome.resized {
                size = session
                    .size()
                    .context("reading terminal size after resize")?;
            }
            if let Ok(actual_size) = session.size()
                && actual_size != size
            {
                size = actual_size;
            }

            let status_text = status_line(size, show_overlay);

            let frame: Cow<'_, str> = if show_overlay {
                Cow::Owned(render_plot_overlay(&scene))
            } else {
                Cow::Borrowed(frame_cache.get_or_render(
                    &scene,
                    request.kind,
                    &state,
                    protocol,
                    size,
                )?)
            };

            if protocol == Protocol::Blocks || show_overlay {
                session.draw_frame(frame.as_ref(), &status_text)?;
                last_protocol_chrome_size = None;
            } else {
                let repaint_static_chrome = last_protocol_chrome_size != Some(size);
                let chrome = plot_protocol_chrome(
                    frame.as_ref(),
                    &scene,
                    &state,
                    protocol,
                    size,
                    &status_text,
                    repaint_static_chrome,
                );
                session.draw_plot_protocol_frame(chrome)?;
                last_protocol_chrome_size = Some(size);
            }
            dirty = false;
        }

        if let Some(event) = session.read_event()? {
            let outcome = handle_plot_event(event, &mut size, &mut state, &mut show_overlay);
            if outcome.quit {
                break;
            }
            dirty = dirty || outcome.dirty;
        }
    }

    Ok(())
}

fn resolve_plot_protocol(protocol: Protocol) -> Protocol {
    match protocol {
        Protocol::Auto => crate::render::terminal::detect(Protocol::Auto).preferred,
        Protocol::Blocks | Protocol::Kitty => protocol,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PlotViewState {
    full: PlotBounds,
    visible: PlotBounds,
    fit_mode: bool,
}

impl PlotViewState {
    fn new(full: PlotBounds) -> Self {
        Self {
            full,
            visible: full,
            fit_mode: true,
        }
    }

    fn pan_left(&mut self) {
        self.pan_horizontal(-1.0);
    }

    fn pan_right(&mut self) {
        self.pan_horizontal(1.0);
    }

    fn pan_up(&mut self) {
        self.pan_vertical(1.0);
    }

    fn pan_down(&mut self) {
        self.pan_vertical(-1.0);
    }

    fn pan_horizontal(&mut self, direction: f64) {
        let span = (self.visible.x_max - self.visible.x_min)
            .abs()
            .max(MIN_SPAN);
        let step = span * PAN_STEP;
        let next_start = self.visible.x_min + step * direction;
        let previous = self.visible;
        self.visible.x_min = next_start;
        self.visible.x_max = next_start + span;
        self.clamp_visible_x();
        self.fit_mode = spans_are_full(self.visible, self.full);
        if self.visible != previous {
            self.fit_mode = false;
        }
    }

    fn pan_vertical(&mut self, direction: f64) {
        let span = (self.visible.y_max - self.visible.y_min)
            .abs()
            .max(MIN_SPAN);
        let step = span * PAN_STEP;
        let next_start = self.visible.y_min + step * direction;
        let previous = self.visible;
        self.visible.y_min = next_start;
        self.visible.y_max = next_start + span;
        self.clamp_visible_y();
        self.fit_mode = spans_are_full(self.visible, self.full);
        if self.visible != previous {
            self.fit_mode = false;
        }
    }

    fn zoom_in(&mut self) {
        self.zoom(ZOOM_STEP);
    }

    fn zoom_out(&mut self) {
        self.zoom(1.0 / ZOOM_STEP);
    }

    fn reset(&mut self) {
        self.visible = self.full;
        self.fit_mode = true;
    }

    fn zoom(&mut self, factor: f64) {
        if factor <= 0.0 {
            return;
        }

        let full_x = (self.full.x_max - self.full.x_min).abs().max(MIN_SPAN);
        let full_y = (self.full.y_max - self.full.y_min).abs().max(MIN_SPAN);
        let current_x = (self.visible.x_max - self.visible.x_min)
            .abs()
            .max(MIN_SPAN);
        let current_y = (self.visible.y_max - self.visible.y_min)
            .abs()
            .max(MIN_SPAN);

        let next_x = (current_x / factor).clamp(MIN_SPAN, full_x);
        let next_y = (current_y / factor).clamp(MIN_SPAN, full_y);

        let x_center = (self.visible.x_min + self.visible.x_max) / 2.0;
        let y_center = (self.visible.y_min + self.visible.y_max) / 2.0;

        self.visible.x_min = x_center - next_x / 2.0;
        self.visible.x_max = x_center + next_x / 2.0;
        self.visible.y_min = y_center - next_y / 2.0;
        self.visible.y_max = y_center + next_y / 2.0;
        self.clamp_visible_x();
        self.clamp_visible_y();
        self.fit_mode = spans_are_full(self.visible, self.full);
    }

    fn clamp_visible_x(&mut self) {
        self.visible.x_min = min_value(self.visible.x_min, self.visible.x_max).0;
        self.visible.x_max = min_value(self.visible.x_min, self.visible.x_max).1;
        let (next_min, next_max) = clamp_axis_window(
            self.visible.x_min,
            self.visible.x_max,
            self.full.x_min,
            self.full.x_max,
        );
        self.visible.x_min = next_min;
        self.visible.x_max = next_max;
    }

    fn clamp_visible_y(&mut self) {
        self.visible.y_min = min_value(self.visible.y_min, self.visible.y_max).0;
        self.visible.y_max = min_value(self.visible.y_min, self.visible.y_max).1;
        let (next_min, next_max) = clamp_axis_window(
            self.visible.y_min,
            self.visible.y_max,
            self.full.y_min,
            self.full.y_max,
        );
        self.visible.y_min = next_min;
        self.visible.y_max = next_max;
    }
}

fn min_value(first: f64, second: f64) -> (f64, f64) {
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
}

fn spans_are_full(visible: PlotBounds, full: PlotBounds) -> bool {
    let full_x = (full.x_max - full.x_min).abs().max(MIN_SPAN);
    let full_y = (full.y_max - full.y_min).abs().max(MIN_SPAN);
    let visible_x = (visible.x_max - visible.x_min).abs().abs().max(MIN_SPAN);
    let visible_y = (visible.y_max - visible.y_min).abs().abs().max(MIN_SPAN);
    (visible_x - full_x).abs() < 1e-9 && (visible_y - full_y).abs() < 1e-9
}

fn clamp_axis_window(start: f64, end: f64, full_min: f64, full_max: f64) -> (f64, f64) {
    let (start, end) = min_value(start, end);
    let (full_min, full_max) = min_value(full_min, full_max);
    let span = (end - start).abs().max(MIN_SPAN);
    let full_span = (full_max - full_min).abs().max(MIN_SPAN);

    if span >= full_span {
        return (full_min, full_max);
    }

    let mut next_start = start.clamp(full_min, full_max);
    let mut next_end = next_start + span;
    if next_end > full_max {
        next_end = full_max;
        next_start = next_end - span;
    }

    (next_start, next_end)
}

fn load_scene(
    source: &InputSource,
    profile: &InputProfile,
    x: Option<&str>,
    y: Option<&str>,
    group: Option<&str>,
) -> Result<PlotScene> {
    match profile.content {
        ContentKind::Csv => {
            table::load_scene(source, x, y, group, b',').context("loading plot scene from CSV data")
        }
        ContentKind::Tsv => table::load_scene(source, x, y, group, b'\t')
            .context("loading plot scene from TSV data"),
        ContentKind::Jsonl => {
            stream::load_scene(source, x, y, group).context("loading plot scene from jsonl data")
        }
        _ => bail!("plot viewer only supports csv, tsv, and jsonl inputs"),
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct PlotEventOutcome {
    dirty: bool,
    quit: bool,
    resized: bool,
}

fn drain_pending_plot_events(
    session: &mut TerminalSession,
    size: &mut TerminalSize,
    state: &mut PlotViewState,
    show_overlay: &mut bool,
) -> Result<PlotEventOutcome> {
    let mut outcome = PlotEventOutcome::default();
    for _ in 0..MAX_PENDING_EVENTS_PER_FRAME {
        let Some(event) = session.read_pending_event()? else {
            break;
        };
        let next = handle_plot_event(event, size, state, show_overlay);
        outcome.dirty |= next.dirty;
        outcome.resized |= next.resized;
        if next.quit {
            outcome.quit = true;
            break;
        }
    }
    Ok(outcome)
}

fn handle_plot_event(
    event: Event,
    size: &mut TerminalSize,
    state: &mut PlotViewState,
    show_overlay: &mut bool,
) -> PlotEventOutcome {
    match event {
        Event::Resize(cols, rows) => {
            let previous_size = *size;
            size.width = cols.max(1);
            size.height = rows.max(1);
            PlotEventOutcome {
                dirty: *size != previous_size,
                resized: *size != previous_size,
                quit: false,
            }
        }
        Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
            let previous_state = *state;
            let previous_overlay = *show_overlay;
            match key_event.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    return PlotEventOutcome {
                        dirty: false,
                        quit: true,
                        resized: false,
                    };
                }
                KeyCode::Left => state.pan_left(),
                KeyCode::Right => state.pan_right(),
                KeyCode::Up => state.pan_up(),
                KeyCode::Down => state.pan_down(),
                KeyCode::Char('+') | KeyCode::Char('=') => state.zoom_in(),
                KeyCode::Char('-') => state.zoom_out(),
                KeyCode::Char('0') => state.reset(),
                KeyCode::Char('m') | KeyCode::Char('M') => {
                    *show_overlay = !*show_overlay;
                }
                _ => {}
            }
            PlotEventOutcome {
                dirty: *state != previous_state || *show_overlay != previous_overlay,
                quit: false,
                resized: false,
            }
        }
        _ => PlotEventOutcome::default(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PlotFrameCacheKey {
    kind: PlotKind,
    protocol: Protocol,
    visible: PlotBounds,
    size: TerminalSize,
}

#[derive(Debug, Default)]
struct PlotFrameCache {
    last: Option<CachedPlotFrame>,
}

#[derive(Debug)]
struct CachedPlotFrame {
    key: PlotFrameCacheKey,
    frame: String,
}

impl PlotFrameCache {
    fn get_or_render(
        &mut self,
        scene: &PlotScene,
        kind: PlotKind,
        state: &PlotViewState,
        protocol: Protocol,
        size: TerminalSize,
    ) -> Result<&str> {
        let key = PlotFrameCacheKey {
            kind,
            protocol,
            visible: state.visible,
            size,
        };
        let cache_hit = self.last.as_ref().is_some_and(|cached| cached.key == key);
        if cache_hit {
            return Ok(self
                .last
                .as_ref()
                .map(|cached| cached.frame.as_str())
                .unwrap_or_default());
        }

        let frame = render_plot_frame(scene, kind, state, protocol, size)?;
        self.last = Some(CachedPlotFrame { key, frame });
        Ok(self
            .last
            .as_ref()
            .map(|cached| cached.frame.as_str())
            .unwrap_or_default())
    }
}

fn render_plot_frame(
    scene: &PlotScene,
    kind: PlotKind,
    state: &PlotViewState,
    protocol: Protocol,
    size: TerminalSize,
) -> Result<String> {
    let cols = u32::from(size.width);
    let rows = u32::from(size.height.saturating_sub(1)).max(1);
    if cols == 0 || rows == 0 {
        return Ok(String::new());
    }

    if protocol == Protocol::Blocks {
        let drawable_cols = u32::from(size.width.saturating_sub(1).max(1));
        return blocks::render_terminal_plot_for_size(
            scene,
            kind,
            state.visible,
            drawable_cols,
            rows,
        )
        .context("rendering terminal plot frame");
    }

    let layout = plot_protocol_layout(size);
    let target = pixel_protocol_target_size(
        protocol,
        u32::from(layout.image_cols),
        u32::from(layout.image_rows),
    );
    let image = crate::render::protocols::plot::render_interactive_plot_body_rgba_for_size(
        scene,
        kind,
        state.visible,
        target.0,
        target.1,
    )?;
    let context = ProtocolRenderContext::new(protocol);
    Ok(render_plot_rgba_with_fallback(
        context,
        &image,
        u32::from(layout.image_cols),
        u32::from(layout.image_rows),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PlotProtocolLayout {
    image_col: u16,
    image_row: u16,
    image_cols: u16,
    image_rows: u16,
    x_axis_row: u16,
}

fn plot_protocol_layout(size: TerminalSize) -> PlotProtocolLayout {
    let header_rows = if size.height >= 8 { 2 } else { 1 };
    let x_axis_rows = if size.height >= 8 { 1 } else { 0 };
    let status_rows = 1;
    let y_axis_cols = if size.width >= 56 { 9 } else { 0 };
    let reserved_rows = header_rows + x_axis_rows + status_rows;
    let image_rows = size.height.saturating_sub(reserved_rows).max(1);
    let image_cols = size.width.saturating_sub(y_axis_cols).max(1);
    let image_row = header_rows.min(size.height.saturating_sub(1));
    let x_axis_row = image_row
        .saturating_add(image_rows)
        .min(size.height.saturating_sub(2));

    PlotProtocolLayout {
        image_col: y_axis_cols,
        image_row,
        image_cols,
        image_rows,
        x_axis_row,
    }
}

fn plot_protocol_chrome<'a>(
    payload: &'a str,
    scene: &PlotScene,
    state: &PlotViewState,
    protocol: Protocol,
    size: TerminalSize,
    status: &'a str,
    repaint_static_chrome: bool,
) -> PlotProtocolFrame<'a> {
    let layout = plot_protocol_layout(size);
    PlotProtocolFrame {
        payload,
        repaint_static_chrome,
        image_col: layout.image_col,
        image_row: layout.image_row,
        image_cols: layout.image_cols,
        image_rows: layout.image_rows,
        x_axis_row: layout.x_axis_row,
        header: plot_header(scene, state, protocol),
        legend: plot_legend(scene, size.width),
        y_labels: plot_y_labels(state.visible, layout),
        x_labels: plot_x_labels(state.visible, layout, size.width),
        status,
    }
}

fn plot_header(scene: &PlotScene, state: &PlotViewState, protocol: Protocol) -> String {
    let title = scene.title.as_deref().unwrap_or("plot");
    let mode = if state.fit_mode { "fit" } else { "pan/zoom" };
    format!(
        "{title} · {} · {mode} · {} series · {} pts · x {:.3}-{:.3} · y {:.3}-{:.3}",
        protocol_label(protocol),
        scene.series.len(),
        scene.total_points(),
        state.visible.x_min,
        state.visible.x_max,
        state.visible.y_min,
        state.visible.y_max,
    )
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
            r: 96,
            g: 165,
            b: 250,
        },
        Color::Rgb {
            r: 251,
            g: 146,
            b: 60,
        },
        Color::Rgb {
            r: 52,
            g: 211,
            b: 153,
        },
        Color::Rgb {
            r: 248,
            g: 113,
            b: 113,
        },
        Color::Rgb {
            r: 196,
            g: 181,
            b: 253,
        },
        Color::Rgb {
            r: 244,
            g: 114,
            b: 182,
        },
    ];
    COLORS[index % COLORS.len()]
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

fn pixel_protocol_target_size(protocol: Protocol, cols: u32, rows: u32) -> (u32, u32) {
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

fn status_line(size: TerminalSize, show_overlay: bool) -> String {
    let overlay_hint = if show_overlay { "m chart" } else { "m info" };
    let status = format!("arrows pan · +/- zoom · 0 fit · {overlay_hint} · q quit");
    trim_to_width(&status, usize::from(size.width.max(1)))
}

fn protocol_label(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Auto => "auto",
        Protocol::Kitty => "kitty",
        Protocol::Blocks => "blocks",
    }
}

fn render_plot_overlay(scene: &PlotScene) -> String {
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

pub(crate) struct PlotRequest {
    pub(crate) x: Option<String>,
    pub(crate) y: Option<String>,
    pub(crate) group: Option<String>,
    pub(crate) kind: PlotKind,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plot::model::{PlotPoint, PlotSeries};
    use crate::{input::InputSource, profile::InputProfile};
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use flate2::read::ZlibDecoder;
    use image::RgbaImage;
    use std::{
        hint::black_box,
        io::{Read, Write},
        path::Path,
        time::{Duration, Instant},
    };

    #[test]
    fn status_line_is_control_only() {
        let size = TerminalSize {
            width: 200,
            height: 24,
        };
        let normal = status_line(size, false);
        let overlay = status_line(size, true);

        assert!(normal.contains("arrows pan"));
        assert!(normal.contains("+/- zoom"));
        assert!(normal.contains("q quit"));
        assert!(!normal.contains("kitty"));
        assert!(!normal.contains("series"));
        assert!(!normal.contains("pts"));
        assert!(overlay.contains("m chart"));
    }

    #[test]
    fn blocks_plot_frame_does_not_emit_white_background_bands() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: vec![
                    PlotPoint { x: 1.0, y: 118.0 },
                    PlotPoint { x: 2.0, y: 121.0 },
                    PlotPoint { x: 3.0, y: 125.0 },
                ],
            }],
        };
        let state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let size = TerminalSize {
            width: 120,
            height: 32,
        };

        let frame =
            render_plot_frame(&scene, PlotKind::Line, &state, Protocol::Blocks, size).unwrap();

        assert!(!frame.contains("255;255;255"));
        assert!(!frame.contains('▀'));
        assert!(!frame.contains('▄'));
        assert!(!frame.contains('●'));
        assert!(!frame.contains('○'));
        assert!(frame.contains("13;17;23"));
        assert!(frame.contains("38;2;148;163;184"));
        assert!(frame.contains("api"));
        assert!(frame.contains("125.00"));
        assert!(frame.contains("3.000"));
        assert!(contains_braille(&frame));
        assert!(frame.contains('*'));

        let plain = strip_ansi(&frame);
        assert!(
            plain
                .lines()
                .all(|line| line.chars().count() < usize::from(size.width))
        );
    }

    #[test]
    fn image_protocol_plot_frame_renders_raster_payload() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: vec![
                    PlotPoint { x: 1.0, y: 118.0 },
                    PlotPoint { x: 2.0, y: 121.0 },
                    PlotPoint { x: 3.0, y: 125.0 },
                ],
            }],
        };
        let state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let size = TerminalSize {
            width: 120,
            height: 32,
        };

        let frame =
            render_plot_frame(&scene, PlotKind::Line, &state, Protocol::Kitty, size).unwrap();

        assert!(frame.contains("\x1b_G"));
        assert!(!frame.contains("13;17;23"));
        assert!(!contains_braille(&frame));
    }

    #[test]
    fn kitty_plot_frame_encodes_dark_interactive_rgba() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: vec![
                    PlotPoint { x: 1.0, y: 118.0 },
                    PlotPoint { x: 2.0, y: 121.0 },
                    PlotPoint { x: 3.0, y: 125.0 },
                ],
            }],
        };
        let state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let size = TerminalSize {
            width: 120,
            height: 32,
        };

        let frame =
            render_plot_frame(&scene, PlotKind::Line, &state, Protocol::Kitty, size).unwrap();
        let image = decode_first_kitty_rgba_payload(&frame);

        assert_eq!(
            (image.width(), image.height()),
            expected_protocol_body_pixels(Protocol::Kitty, size)
        );
        assert_eq!(image.get_pixel(0, 0).0, [13, 17, 23, 255]);
    }

    #[test]
    fn kitty_plot_target_keeps_normal_terminal_size_exact() {
        assert_eq!(
            pixel_protocol_target_size(Protocol::Kitty, 120, 31),
            (960, 496)
        );
    }

    #[test]
    fn kitty_plot_target_caps_large_terminal_pixels() {
        let target = pixel_protocol_target_size(Protocol::Kitty, 300, 99);

        assert!(u64::from(target.0) * u64::from(target.1) <= MAX_INTERACTIVE_PLOT_PIXELS);
        assert!(target.0 < 300 * CELL_PIXEL_WIDTH);
        assert!(target.1 < 99 * CELL_PIXEL_HEIGHT);
    }

    #[test]
    fn plot_frame_cache_reuses_same_payload_for_same_key() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: vec![
                    PlotPoint { x: 1.0, y: 118.0 },
                    PlotPoint { x: 2.0, y: 121.0 },
                    PlotPoint { x: 3.0, y: 125.0 },
                ],
            }],
        };
        let state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let size = TerminalSize {
            width: 120,
            height: 32,
        };
        let mut cache = PlotFrameCache::default();

        let first_ptr = cache
            .get_or_render(&scene, PlotKind::Line, &state, Protocol::Kitty, size)
            .unwrap()
            .as_ptr() as usize;
        let second_ptr = cache
            .get_or_render(&scene, PlotKind::Line, &state, Protocol::Kitty, size)
            .unwrap()
            .as_ptr() as usize;

        assert_eq!(first_ptr, second_ptr);
        assert_eq!(cache.last.as_ref().unwrap().key.size, size);
    }

    #[test]
    fn plot_frame_cache_rerenders_for_resized_target() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: vec![
                    PlotPoint { x: 1.0, y: 118.0 },
                    PlotPoint { x: 2.0, y: 121.0 },
                    PlotPoint { x: 3.0, y: 125.0 },
                ],
            }],
        };
        let state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let mut cache = PlotFrameCache::default();

        let small = cache
            .get_or_render(
                &scene,
                PlotKind::Line,
                &state,
                Protocol::Kitty,
                TerminalSize {
                    width: 80,
                    height: 24,
                },
            )
            .unwrap()
            .to_owned();
        let large = cache
            .get_or_render(
                &scene,
                PlotKind::Line,
                &state,
                Protocol::Kitty,
                TerminalSize {
                    width: 120,
                    height: 32,
                },
            )
            .unwrap()
            .to_owned();

        let small_image = decode_first_kitty_rgba_payload(&small);
        let large_image = decode_first_kitty_rgba_payload(&large);

        assert_eq!(
            (small_image.width(), small_image.height()),
            expected_protocol_body_pixels(
                Protocol::Kitty,
                TerminalSize {
                    width: 80,
                    height: 24
                }
            )
        );
        assert_eq!(
            (large_image.width(), large_image.height()),
            expected_protocol_body_pixels(
                Protocol::Kitty,
                TerminalSize {
                    width: 120,
                    height: 32
                }
            )
        );
        assert_eq!(
            cache.last.as_ref().unwrap().key.size,
            TerminalSize {
                width: 120,
                height: 32
            }
        );
    }

    #[test]
    fn resize_event_marks_plot_frame_dirty() {
        let mut size = TerminalSize {
            width: 80,
            height: 24,
        };
        let mut state = PlotViewState::new(PlotBounds {
            x_min: 0.0,
            x_max: 10.0,
            y_min: 0.0,
            y_max: 10.0,
        });
        let mut show_overlay = false;

        let outcome = handle_plot_event(
            Event::Resize(120, 40),
            &mut size,
            &mut state,
            &mut show_overlay,
        );

        assert!(outcome.dirty);
        assert!(outcome.resized);
        assert_eq!(
            size,
            TerminalSize {
                width: 120,
                height: 40
            }
        );
    }

    #[test]
    fn plot_frame_renders_every_explicit_protocol() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: vec![
                    PlotPoint { x: 1.0, y: 118.0 },
                    PlotPoint { x: 2.0, y: 121.0 },
                    PlotPoint { x: 3.0, y: 125.0 },
                ],
            }],
        };
        let state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let size = TerminalSize {
            width: 80,
            height: 24,
        };
        let cases = [(Protocol::Blocks, "13;17;23"), (Protocol::Kitty, "\x1b_G")];

        for (protocol, marker) in cases {
            let frame = render_plot_frame(&scene, PlotKind::Line, &state, protocol, size)
                .unwrap_or_else(|error| panic!("{protocol:?} plot frame failed: {error}"));

            assert!(
                frame.contains(marker),
                "{protocol:?} plot frame did not include marker {marker:?}"
            );
        }
    }

    #[test]
    fn plot_protocol_chrome_promotes_labels_outside_image_payload() {
        let scene = PlotScene {
            title: Some("latency.csv".to_owned()),
            series: vec![
                PlotSeries {
                    name: "api".to_owned(),
                    points: vec![PlotPoint { x: 1.0, y: 118.0 }],
                },
                PlotSeries {
                    name: "worker".to_owned(),
                    points: vec![PlotPoint { x: 2.0, y: 134.0 }],
                },
            ],
        };
        let state = PlotViewState::new(PlotBounds {
            x_min: 1.0,
            x_max: 20.0,
            y_min: 118.0,
            y_max: 205.0,
        });
        let size = TerminalSize {
            width: 120,
            height: 32,
        };
        let chrome = plot_protocol_chrome(
            "payload",
            &scene,
            &state,
            Protocol::Kitty,
            size,
            "status",
            true,
        );

        assert_eq!(chrome.image_col, 9);
        assert_eq!(chrome.image_row, 2);
        assert_eq!(chrome.image_cols, 111);
        assert_eq!(chrome.image_rows, 28);
        assert_eq!(chrome.x_axis_row, 30);
        assert!(chrome.header.contains("latency.csv"));
        assert!(chrome.header.contains("kitty"));
        assert_eq!(chrome.legend.len(), 2);
        assert!(chrome.legend.iter().all(|item| item.marker == "━━"));
        assert!(
            chrome
                .y_labels
                .iter()
                .any(|label| label.text.contains("205"))
        );
        assert!(
            chrome
                .x_labels
                .iter()
                .any(|label| label.text.contains("20"))
        );
    }

    #[test]
    #[ignore = "local perf test; run scripts/bench-plot-recompute.sh"]
    fn plot_recompute_perf() {
        let iterations = std::env::var("TERMVIZ_PLOT_RECOMPUTE_ITERS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(12);
        let scene = dense_perf_scene();
        let base_state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let small_size = TerminalSize {
            width: 80,
            height: 24,
        };
        let large_size = TerminalSize {
            width: 120,
            height: 32,
        };

        print_detailed_perf_metric("kitty_uncached_120x32", iterations, || {
            vec![
                profile_pixel_plot_frame(
                    black_box(&scene),
                    PlotKind::Line,
                    black_box(&base_state),
                    Protocol::Kitty,
                    large_size,
                )
                .expect("profile kitty plot frame"),
            ]
        });

        print_detailed_perf_metric("kitty_resize_80x24_120x32", iterations, || {
            let small = profile_pixel_plot_frame(
                black_box(&scene),
                PlotKind::Line,
                black_box(&base_state),
                Protocol::Kitty,
                small_size,
            )
            .expect("profile small kitty plot frame");
            let large = profile_pixel_plot_frame(
                black_box(&scene),
                PlotKind::Line,
                black_box(&base_state),
                Protocol::Kitty,
                large_size,
            )
            .expect("profile large kitty plot frame");
            vec![small, large]
        });

        let mut cache = PlotFrameCache::default();
        let _ = cache
            .get_or_render(
                black_box(&scene),
                PlotKind::Line,
                black_box(&base_state),
                Protocol::Kitty,
                large_size,
            )
            .expect("prime plot frame cache");
        print_detailed_perf_metric("kitty_cache_hit_120x32", iterations, || {
            vec![
                profile_cached_plot_frame(
                    &mut cache,
                    black_box(&scene),
                    PlotKind::Line,
                    black_box(&base_state),
                    Protocol::Kitty,
                    large_size,
                )
                .expect("profile plot frame cache hit"),
            ]
        });

        print_detailed_perf_metric("kitty_pan_burst_120x32", iterations, || {
            let mut state = base_state;
            let mut samples = Vec::new();
            state.zoom_in();
            for _ in 0..8 {
                state.pan_right();
                let sample = profile_pixel_plot_frame(
                    black_box(&scene),
                    PlotKind::Line,
                    black_box(&state),
                    Protocol::Kitty,
                    large_size,
                )
                .expect("profile panned kitty plot frame");
                samples.push(sample);
            }
            samples
        });

        print_detailed_perf_metric("blocks_uncached_120x32", iterations, || {
            vec![
                profile_blocks_plot_frame(
                    black_box(&scene),
                    PlotKind::Line,
                    black_box(&base_state),
                    Protocol::Blocks,
                    large_size,
                )
                .expect("profile blocks plot frame"),
            ]
        });

        let mut file = tempfile::NamedTempFile::with_suffix(".csv").unwrap();
        write_dense_perf_csv(&mut file);
        print_detailed_perf_metric("kitty_full_pipeline_120x32", iterations, || {
            vec![
                profile_plot_startup_and_frame(file.path(), Protocol::Kitty, large_size)
                    .expect("profile full kitty plot pipeline"),
            ]
        });
        print_detailed_perf_metric("blocks_full_pipeline_120x32", iterations, || {
            vec![
                profile_plot_startup_and_frame(file.path(), Protocol::Blocks, large_size)
                    .expect("profile full blocks plot pipeline"),
            ]
        });
    }

    #[test]
    fn plot_overlay_contains_point_series_and_bounds() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "svc-a".to_owned(),
                points: vec![PlotPoint { x: 1.0, y: 20.0 }, PlotPoint { x: 2.0, y: 40.0 }],
            }],
        };

        let overlay = render_plot_overlay(&scene);
        assert!(overlay.contains("points: 2"));
        assert!(overlay.contains("series: 1"));
        assert!(overlay.contains("x: [1.000, 2.000]"));
        assert!(overlay.contains("y: [20.000, 40.000]"));
    }

    #[test]
    fn plot_view_state_can_pan_and_zoom() {
        let mut state = PlotViewState::new(PlotBounds {
            x_min: 0.0,
            x_max: 100.0,
            y_min: 0.0,
            y_max: 100.0,
        });
        assert_eq!(state.visible.x_min, 0.0);
        assert!(state.fit_mode);

        state.zoom_in();
        assert!(!state.fit_mode);
        state.pan_right();
        assert!(!state.fit_mode);
        assert!(state.visible.x_min > 0.0);

        let current_span_x = state.visible.x_max - state.visible.x_min;
        state.zoom_in();
        assert!(state.visible.x_max - state.visible.x_min < current_span_x);

        state.reset();
        assert!(state.fit_mode);
        assert_eq!(state.visible.x_min, 0.0);
        assert_eq!(state.visible.x_max, 100.0);
    }

    #[derive(Debug, Clone)]
    struct PlotPerfSample {
        total: Duration,
        profile: Duration,
        load: Duration,
        layout: Duration,
        display_list: Duration,
        raster: Duration,
        compose: Duration,
        protocol: Duration,
        chrome: Duration,
        bytes: usize,
        chrome_bytes: usize,
        commands: usize,
        image_pixels: u64,
    }

    fn profile_pixel_plot_frame(
        scene: &PlotScene,
        kind: PlotKind,
        state: &PlotViewState,
        protocol: Protocol,
        size: TerminalSize,
    ) -> Result<PlotPerfSample> {
        assert_ne!(protocol, Protocol::Blocks);
        let total_start = Instant::now();
        let layout_start = Instant::now();
        let layout = plot_protocol_layout(size);
        let target = pixel_protocol_target_size(
            protocol,
            u32::from(layout.image_cols),
            u32::from(layout.image_rows),
        );
        let layout_time = layout_start.elapsed();

        let timed = crate::render::protocols::plot::render_interactive_plot_body_timed_for_size(
            scene,
            kind,
            state.visible,
            target.0,
            target.1,
        )?;

        let protocol_start = Instant::now();
        let payload = render_plot_rgba_with_fallback(
            ProtocolRenderContext::new(protocol),
            &timed.image,
            u32::from(layout.image_cols),
            u32::from(layout.image_rows),
        );
        let protocol_time = protocol_start.elapsed();

        let chrome_start = Instant::now();
        let status = status_line(size, false);
        let chrome = plot_protocol_chrome(&payload, scene, state, protocol, size, &status, false);
        let chrome_bytes = estimate_plot_chrome_bytes(&chrome, size);
        let chrome_time = chrome_start.elapsed();

        Ok(PlotPerfSample {
            total: total_start.elapsed(),
            profile: Duration::ZERO,
            load: Duration::ZERO,
            layout: layout_time,
            display_list: timed.display_list,
            raster: timed.raster,
            compose: Duration::ZERO,
            protocol: protocol_time,
            chrome: chrome_time,
            bytes: payload.len(),
            chrome_bytes,
            commands: timed.command_count,
            image_pixels: u64::from(timed.image.width()) * u64::from(timed.image.height()),
        })
    }

    fn profile_blocks_plot_frame(
        scene: &PlotScene,
        kind: PlotKind,
        state: &PlotViewState,
        protocol: Protocol,
        size: TerminalSize,
    ) -> Result<PlotPerfSample> {
        assert_eq!(protocol, Protocol::Blocks);
        let start = Instant::now();
        let payload = render_plot_frame(scene, kind, state, protocol, size)?;
        Ok(PlotPerfSample {
            total: start.elapsed(),
            profile: Duration::ZERO,
            load: Duration::ZERO,
            layout: Duration::ZERO,
            display_list: Duration::ZERO,
            raster: Duration::ZERO,
            compose: Duration::ZERO,
            protocol: Duration::ZERO,
            chrome: Duration::ZERO,
            bytes: payload.len(),
            chrome_bytes: 0,
            commands: 0,
            image_pixels: 0,
        })
    }

    fn profile_plot_startup_and_frame(
        path: &Path,
        protocol: Protocol,
        size: TerminalSize,
    ) -> Result<PlotPerfSample> {
        let total_start = Instant::now();

        let profile_start = Instant::now();
        let source = InputSource::from_path(path.to_path_buf())?;
        let profile = InputProfile::resolve(&source, PlotKind::Line, None)?;
        let profile_time = profile_start.elapsed();

        let load_start = Instant::now();
        let scene = load_scene(
            &source,
            &profile,
            Some("time"),
            Some("latency"),
            Some("service"),
        )?;
        let load_time = load_start.elapsed();

        let state = PlotViewState::new(scene.bounds().context("plot scene is empty")?.normalized());
        let mut sample = match protocol {
            Protocol::Kitty => {
                profile_pixel_plot_frame(&scene, PlotKind::Line, &state, protocol, size)?
            }
            Protocol::Blocks => {
                profile_blocks_plot_frame(&scene, PlotKind::Line, &state, protocol, size)?
            }
            Protocol::Auto => unreachable!("auto protocol should be resolved before profiling"),
        };
        sample.profile = profile_time;
        sample.load = load_time;
        sample.total = total_start.elapsed();
        Ok(sample)
    }

    fn profile_cached_plot_frame(
        cache: &mut PlotFrameCache,
        scene: &PlotScene,
        kind: PlotKind,
        state: &PlotViewState,
        protocol: Protocol,
        size: TerminalSize,
    ) -> Result<PlotPerfSample> {
        let start = Instant::now();
        let payload = cache.get_or_render(scene, kind, state, protocol, size)?;
        Ok(PlotPerfSample {
            total: start.elapsed(),
            profile: Duration::ZERO,
            load: Duration::ZERO,
            layout: Duration::ZERO,
            display_list: Duration::ZERO,
            raster: Duration::ZERO,
            compose: Duration::ZERO,
            protocol: Duration::ZERO,
            chrome: Duration::ZERO,
            bytes: payload.len(),
            chrome_bytes: 0,
            commands: 0,
            image_pixels: 0,
        })
    }

    fn print_detailed_perf_metric<F>(name: &str, iterations: usize, mut run: F)
    where
        F: FnMut() -> Vec<PlotPerfSample>,
    {
        let mut total_time = Duration::ZERO;
        let mut profile_time = Duration::ZERO;
        let mut load_time = Duration::ZERO;
        let mut layout_time = Duration::ZERO;
        let mut display_list_time = Duration::ZERO;
        let mut raster_time = Duration::ZERO;
        let mut compose_time = Duration::ZERO;
        let mut protocol_time = Duration::ZERO;
        let mut chrome_time = Duration::ZERO;
        let mut bytes = 0usize;
        let mut chrome_bytes = 0usize;
        let mut commands = 0usize;
        let mut image_pixels = 0u64;
        for _ in 0..iterations {
            for sample in run() {
                total_time += sample.total;
                profile_time += sample.profile;
                load_time += sample.load;
                layout_time += sample.layout;
                display_list_time += sample.display_list;
                raster_time += sample.raster;
                compose_time += sample.compose;
                protocol_time += sample.protocol;
                chrome_time += sample.chrome;
                bytes = bytes.saturating_add(black_box(sample.bytes));
                chrome_bytes = chrome_bytes.saturating_add(black_box(sample.chrome_bytes));
                commands = commands.saturating_add(black_box(sample.commands));
                image_pixels = image_pixels.saturating_add(black_box(sample.image_pixels));
            }
        }

        let total_us = total_time.as_micros();
        let mean_us = total_us / iterations as u128;
        let mean_profile_us = profile_time.as_micros() / iterations as u128;
        let mean_load_us = load_time.as_micros() / iterations as u128;
        let mean_layout_us = layout_time.as_micros() / iterations as u128;
        let mean_display_list_us = display_list_time.as_micros() / iterations as u128;
        let mean_raster_us = raster_time.as_micros() / iterations as u128;
        let mean_compose_us = compose_time.as_micros() / iterations as u128;
        let mean_protocol_us = protocol_time.as_micros() / iterations as u128;
        let mean_chrome_us = chrome_time.as_micros() / iterations as u128;
        let mean_bytes = bytes / iterations;
        let mean_chrome_bytes = chrome_bytes / iterations;
        let mean_commands = commands / iterations;
        let mean_image_pixels = image_pixels / iterations as u64;
        if !name.contains("full_pipeline") {
            println!(
                "plot_recompute_detail,{name},{iterations},{total_us},{mean_us},{mean_display_list_us},{mean_raster_us},{mean_protocol_us},{bytes},{mean_bytes},{mean_commands},{mean_image_pixels}"
            );
        }
        println!(
            "plot_pipeline_detail,{name},{iterations},{total_us},{mean_us},{mean_profile_us},{mean_load_us},{mean_layout_us},{mean_display_list_us},{mean_raster_us},{mean_compose_us},{mean_protocol_us},{mean_chrome_us},{bytes},{mean_bytes},{mean_chrome_bytes},{mean_commands},{mean_image_pixels}"
        );
    }

    fn expected_protocol_body_pixels(protocol: Protocol, size: TerminalSize) -> (u32, u32) {
        let layout = plot_protocol_layout(size);
        pixel_protocol_target_size(
            protocol,
            u32::from(layout.image_cols),
            u32::from(layout.image_rows),
        )
    }

    fn dense_perf_scene() -> PlotScene {
        let series = (0..4)
            .map(|series_index| {
                let points = (0..256)
                    .map(|index| {
                        let x = index as f64;
                        let wave = ((index as f64 + series_index as f64 * 13.0) / 15.0).sin();
                        let trend = index as f64 * (0.12 + series_index as f64 * 0.02);
                        PlotPoint {
                            x,
                            y: 120.0 + trend + wave * (8.0 + series_index as f64 * 3.0),
                        }
                    })
                    .collect();
                PlotSeries {
                    name: format!("svc-{series_index}"),
                    points,
                }
            })
            .collect();

        PlotScene {
            title: Some("perf".to_owned()),
            series,
        }
    }

    fn write_dense_perf_csv(file: &mut tempfile::NamedTempFile) {
        writeln!(file, "time,latency,service").unwrap();
        for point in 0..1_024 {
            let x = point as f64;
            let api = 150.0 + (x / 8.0).sin() * 42.0 + (x / 29.0).cos() * 7.0;
            let worker = 132.0 + (x / 11.0).cos() * 30.0 + (x / 35.0).sin() * 11.0;
            writeln!(file, "{x:.3},{api:.3},api").unwrap();
            writeln!(file, "{x:.3},{worker:.3},worker").unwrap();
        }
    }

    fn estimate_plot_chrome_bytes(
        frame: &crate::tui::PlotProtocolFrame<'_>,
        size: TerminalSize,
    ) -> usize {
        let legend_bytes = if frame.repaint_static_chrome {
            frame
                .legend
                .iter()
                .map(|item| item.marker.len() + item.label.len() + 3)
                .sum::<usize>()
        } else {
            0
        };
        let text_bytes = frame.header.len()
            + frame.status.len()
            + legend_bytes
            + frame
                .y_labels
                .iter()
                .chain(frame.x_labels.iter())
                .map(|label| label.text.len())
                .sum::<usize>();
        let background_cells = if frame.repaint_static_chrome {
            usize::from(frame.image_row) * usize::from(size.width)
                + usize::from(frame.image_col) * usize::from(frame.image_rows)
                + usize::from(size.width)
        } else {
            0
        };
        text_bytes + background_cells
    }

    #[test]
    fn plot_view_state_keeps_fit_when_pan_has_no_room() {
        let mut state = PlotViewState::new(PlotBounds {
            x_min: 0.0,
            x_max: 100.0,
            y_min: 0.0,
            y_max: 100.0,
        });

        state.pan_right();
        state.pan_up();

        assert!(state.fit_mode);
        assert_eq!(state.visible, state.full);
    }

    #[test]
    fn plot_view_state_vertical_pan_matches_data_direction() {
        let mut state = PlotViewState::new(PlotBounds {
            x_min: 0.0,
            x_max: 100.0,
            y_min: 0.0,
            y_max: 100.0,
        });
        state.zoom_in();
        let after_zoom = state.visible;

        state.pan_up();
        assert!(state.visible.y_min > after_zoom.y_min);

        let after_pan_up = state.visible;
        state.pan_down();
        assert!(state.visible.y_min < after_pan_up.y_min);
    }

    fn strip_ansi(text: &str) -> String {
        let mut output = String::new();
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
            output.push(ch);
        }
        output
    }

    fn contains_braille(text: &str) -> bool {
        text.chars()
            .any(|ch| ('\u{2801}'..='\u{28ff}').contains(&ch))
    }

    fn decode_first_kitty_rgba_payload(payload: &str) -> RgbaImage {
        let mut base64_payload = String::new();
        let mut first_control = None;
        for packet in payload.split("\x1b_G") {
            let Some(packet) = packet.strip_suffix("\x1b\\") else {
                continue;
            };
            let (control, chunk) = packet
                .split_once(';')
                .expect("kitty packet should separate control data and base64");
            if control.contains("t=f") {
                let path = String::from_utf8(STANDARD.decode(chunk).unwrap()).unwrap();
                return image::load_from_memory(&std::fs::read(path).unwrap())
                    .unwrap()
                    .to_rgba8();
            }
            first_control.get_or_insert_with(|| control.to_owned());
            base64_payload.push_str(chunk);
        }
        assert!(!base64_payload.is_empty());
        let control = first_control.expect("kitty payload should include control data");
        let decoded = STANDARD.decode(base64_payload).unwrap();
        if control.contains("f=100") {
            return image::load_from_memory(&decoded).unwrap().to_rgba8();
        }

        let bytes_per_pixel = if control.contains("f=24") {
            3
        } else if control.contains("f=32") {
            4
        } else {
            panic!("expected RGB, RGBA, or PNG kitty payload, got {control}");
        };
        let width = parse_kitty_control_u32(&control, "s").expect("kitty payload width");
        let height = parse_kitty_control_u32(&control, "v").expect("kitty payload height");
        let pixels = if control.contains("o=z") {
            let mut decoder = ZlibDecoder::new(decoded.as_slice());
            let mut output = Vec::new();
            decoder.read_to_end(&mut output).unwrap();
            output
        } else {
            decoded
        };
        assert_eq!(
            pixels.len(),
            width as usize * height as usize * bytes_per_pixel
        );
        let rgba = if bytes_per_pixel == 3 {
            let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
            for pixel in pixels.chunks_exact(3) {
                rgba.extend_from_slice(pixel);
                rgba.push(255);
            }
            rgba
        } else {
            pixels
        };
        RgbaImage::from_raw(width, height, rgba).expect("valid RGBA payload")
    }

    fn parse_kitty_control_u32(control: &str, key: &str) -> Option<u32> {
        control.split(',').find_map(|part| {
            let (candidate, value) = part.split_once('=')?;
            (candidate == key).then(|| value.parse().ok()).flatten()
        })
    }
}
