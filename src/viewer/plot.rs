use std::borrow::Cow;

use anyhow::{Context, Result, bail};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use unicode_width::UnicodeWidthChar;

use crate::{
    input::InputSource,
    plot::model::{PlotBounds, PlotScene},
    plot::{PlotKind, stream, table},
    profile::{ContentKind, InputProfile},
    render::{
        Protocol,
        protocols::{ProtocolRenderContext, blocks, render_raster},
    },
    tui::{TerminalSession, TerminalSize},
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

            let status_text = status_line(&state, &scene, protocol, size, show_overlay);

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
            } else {
                session.draw_protocol_frame(frame.as_ref(), &status_text)?;
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
        Protocol::Auto => Protocol::Blocks,
        Protocol::Blocks | Protocol::Kitty | Protocol::Sixel | Protocol::Iterm => protocol,
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

    let target = pixel_protocol_target_size(protocol, cols, rows);
    let image = crate::render::protocols::plot::render_interactive_plot_for_size(
        scene,
        kind,
        state.visible,
        target.0,
        target.1,
    )?;
    let context = ProtocolRenderContext::new(protocol);
    render_raster(context, &image, cols, rows).context("rendering plot raster payload")
}

fn pixel_protocol_target_size(protocol: Protocol, cols: u32, rows: u32) -> (u32, u32) {
    let width = cols.max(1).saturating_mul(CELL_PIXEL_WIDTH);
    let height = rows.max(1).saturating_mul(CELL_PIXEL_HEIGHT);
    match protocol {
        Protocol::Kitty | Protocol::Iterm => cap_pixel_target(width, height),
        Protocol::Sixel => (width, height),
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

fn status_line(
    state: &PlotViewState,
    scene: &PlotScene,
    protocol: Protocol,
    size: TerminalSize,
    show_overlay: bool,
) -> String {
    let mode = if state.fit_mode { "fit" } else { "pan/zoom" };
    let overlay_hint = if show_overlay { "m chart" } else { "m info" };
    let protocol = protocol_label(protocol);
    let status = format!(
        "{protocol} · {mode} · {} series · {} pts · x {:.3}-{:.3} · y {:.3}-{:.3} · arrows pan · +/- zoom · 0 fit · {overlay_hint} · q quit",
        scene.series.len(),
        scene.total_points(),
        state.visible.x_min,
        state.visible.x_max,
        state.visible.y_min,
        state.visible.y_max
    );
    trim_to_width(&status, usize::from(size.width.max(1)))
}

fn protocol_label(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Auto => "auto",
        Protocol::Kitty => "kitty",
        Protocol::Sixel => "sixel",
        Protocol::Iterm => "iterm",
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
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use std::{
        hint::black_box,
        time::{Duration, Instant},
    };

    #[test]
    fn status_line_shows_mode_controls_and_counts() {
        let scene = PlotScene {
            title: None,
            series: Vec::new(),
        };
        let state = PlotViewState::new(PlotBounds {
            x_min: 1.0,
            x_max: 2.0,
            y_min: 10.0,
            y_max: 20.0,
        });
        let size = TerminalSize {
            width: 200,
            height: 24,
        };
        let normal = status_line(&state, &scene, Protocol::Blocks, size, false);
        let overlay = status_line(&state, &scene, Protocol::Blocks, size, true);

        assert!(normal.contains("fit"));
        assert!(normal.contains("0 series"));
        assert!(normal.contains("0 pts"));
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
    fn kitty_plot_frame_encodes_dark_interactive_png() {
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
        let png_bytes = decode_first_kitty_png_payload(&frame);
        let image = image::load_from_memory(&png_bytes).unwrap().to_rgba8();

        assert_eq!(
            (image.width(), image.height()),
            pixel_protocol_target_size(Protocol::Kitty, 120, 31)
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
    fn sixel_plot_target_keeps_full_terminal_pixels() {
        assert_eq!(
            pixel_protocol_target_size(Protocol::Sixel, 300, 99),
            (300 * CELL_PIXEL_WIDTH, 99 * CELL_PIXEL_HEIGHT)
        );
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

        let small_png = image::load_from_memory(&decode_first_kitty_png_payload(&small)).unwrap();
        let large_png = image::load_from_memory(&decode_first_kitty_png_payload(&large)).unwrap();

        assert_eq!(
            (small_png.width(), small_png.height()),
            pixel_protocol_target_size(Protocol::Kitty, 80, 23)
        );
        assert_eq!(
            (large_png.width(), large_png.height()),
            pixel_protocol_target_size(Protocol::Kitty, 120, 31)
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
        let cases = [
            (Protocol::Blocks, "13;17;23"),
            (Protocol::Kitty, "\x1b_G"),
            (Protocol::Sixel, "\x1bPq"),
            (Protocol::Iterm, "\x1b]1337;File"),
        ];

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
        display_list: Duration,
        raster: Duration,
        protocol: Duration,
        bytes: usize,
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
        let cols = u32::from(size.width);
        let rows = u32::from(size.height.saturating_sub(1)).max(1);
        let target = pixel_protocol_target_size(protocol, cols, rows);

        let total_start = Instant::now();
        let timed = crate::render::protocols::plot::render_interactive_plot_timed_for_size(
            scene,
            kind,
            state.visible,
            target.0,
            target.1,
        )?;

        let protocol_start = Instant::now();
        let payload = render_raster(
            ProtocolRenderContext::new(protocol),
            &timed.image,
            cols,
            rows,
        )
        .context("rendering profiled plot raster payload")?;
        let protocol_time = protocol_start.elapsed();

        Ok(PlotPerfSample {
            total: total_start.elapsed(),
            display_list: timed.display_list,
            raster: timed.raster,
            protocol: protocol_time,
            bytes: payload.len(),
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
            display_list: Duration::ZERO,
            raster: Duration::ZERO,
            protocol: Duration::ZERO,
            bytes: payload.len(),
            commands: 0,
            image_pixels: 0,
        })
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
            display_list: Duration::ZERO,
            raster: Duration::ZERO,
            protocol: Duration::ZERO,
            bytes: payload.len(),
            commands: 0,
            image_pixels: 0,
        })
    }

    fn print_detailed_perf_metric<F>(name: &str, iterations: usize, mut run: F)
    where
        F: FnMut() -> Vec<PlotPerfSample>,
    {
        let mut total_time = Duration::ZERO;
        let mut display_list_time = Duration::ZERO;
        let mut raster_time = Duration::ZERO;
        let mut protocol_time = Duration::ZERO;
        let mut bytes = 0usize;
        let mut commands = 0usize;
        let mut image_pixels = 0u64;
        for _ in 0..iterations {
            for sample in run() {
                total_time += sample.total;
                display_list_time += sample.display_list;
                raster_time += sample.raster;
                protocol_time += sample.protocol;
                bytes = bytes.saturating_add(black_box(sample.bytes));
                commands = commands.saturating_add(black_box(sample.commands));
                image_pixels = image_pixels.saturating_add(black_box(sample.image_pixels));
            }
        }

        let total_us = total_time.as_micros();
        let mean_us = total_us / iterations as u128;
        let mean_display_list_us = display_list_time.as_micros() / iterations as u128;
        let mean_raster_us = raster_time.as_micros() / iterations as u128;
        let mean_protocol_us = protocol_time.as_micros() / iterations as u128;
        let mean_bytes = bytes / iterations;
        let mean_commands = commands / iterations;
        let mean_image_pixels = image_pixels / iterations as u64;
        println!(
            "plot_recompute_detail,{name},{iterations},{total_us},{mean_us},{mean_display_list_us},{mean_raster_us},{mean_protocol_us},{bytes},{mean_bytes},{mean_commands},{mean_image_pixels}"
        );
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

    fn decode_first_kitty_png_payload(payload: &str) -> Vec<u8> {
        let mut base64_payload = String::new();
        for packet in payload.split("\x1b_G") {
            let Some(packet) = packet.strip_suffix("\x1b\\") else {
                continue;
            };
            let (control, chunk) = packet
                .split_once(';')
                .expect("kitty packet should separate control data and base64");
            if control.contains("t=f") {
                let path = String::from_utf8(STANDARD.decode(chunk).unwrap()).unwrap();
                return std::fs::read(path).unwrap();
            }
            base64_payload.push_str(chunk);
        }
        assert!(!base64_payload.is_empty());
        STANDARD.decode(base64_payload).unwrap()
    }
}
