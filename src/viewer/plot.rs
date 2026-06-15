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
        protocols::{ProtocolRenderContext, render_raster},
    },
    tui::{TerminalSession, TerminalSize},
};

const PAN_STEP: f64 = 0.15;
const ZOOM_STEP: f64 = 1.2;
const MIN_SPAN: f64 = 1e-6;

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

    loop {
        let status_text = status_line(&state, &scene, protocol, size, show_overlay);

        let frame = if show_overlay {
            render_plot_overlay(&scene)
        } else {
            render_plot_frame(&scene, request.kind, &state, protocol, size)?
        };

        if protocol == Protocol::Blocks || show_overlay {
            session.draw_frame(&frame, &status_text)?;
        } else {
            session.draw_protocol_frame(&frame, &status_text)?;
        }

        match session.read_event()? {
            Some(Event::Resize(cols, rows)) => {
                size.width = cols.max(1);
                size.height = rows.max(1);
            }
            Some(Event::Key(key_event)) if key_event.kind == KeyEventKind::Press => {
                match key_event.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => break,
                    KeyCode::Left => state.pan_left(),
                    KeyCode::Right => state.pan_right(),
                    KeyCode::Up => state.pan_up(),
                    KeyCode::Down => state.pan_down(),
                    KeyCode::Char('+') | KeyCode::Char('=') => state.zoom_in(),
                    KeyCode::Char('-') => state.zoom_out(),
                    KeyCode::Char('0') => state.reset(),
                    KeyCode::Char('m') | KeyCode::Char('M') => {
                        show_overlay = !show_overlay;
                    }
                    _ => {}
                }
            }
            Some(_) | None => {}
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

#[derive(Debug, Clone, Copy)]
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
        ContentKind::Csv | ContentKind::Tsv => {
            table::load_scene(source, x, y, group).context("loading plot scene from table data")
        }
        ContentKind::Jsonl => {
            stream::load_scene(source, x, y, group).context("loading plot scene from jsonl data")
        }
        _ => bail!("plot viewer only supports csv, tsv, and jsonl inputs"),
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

    let image = crate::render::protocols::plot::render_plot_for_bounds(scene, kind, state.visible)?;
    let context = ProtocolRenderContext::new(protocol);
    render_raster(context, &image, cols, rows).context("rendering plot raster payload")
}

fn status_line(
    state: &PlotViewState,
    scene: &PlotScene,
    protocol: Protocol,
    size: TerminalSize,
    show_overlay: bool,
) -> String {
    let mode = if state.fit_mode { "fit" } else { "pan/zoom" };
    let control_hint = if show_overlay {
        "m compact"
    } else {
        "m summary"
    };
    let status = format!(
        "{} | arrows pan zoom +/- 0 fit q quit | {} | mode={} | series={} points={} | x:[{:.3}, {:.3}] y:[{:.3}, {:.3}]",
        crate::render::protocols::protocol_name(protocol),
        control_hint,
        mode,
        scene.series.len(),
        scene.total_points(),
        state.visible.x_min,
        state.visible.x_max,
        state.visible.y_min,
        state.visible.y_max
    );
    trim_to_width(&status, usize::from(size.width.max(1)))
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

        assert!(normal.contains("mode=fit"));
        assert!(normal.contains("series=0"));
        assert!(normal.contains("points=0"));
        assert!(overlay.contains("m compact"));
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
}
