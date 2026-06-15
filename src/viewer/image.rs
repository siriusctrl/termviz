use std::cmp::max;

use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use image::{DynamicImage, ImageReader, imageops::FilterType};
use unicode_width::UnicodeWidthChar;

use crate::{
    input::InputSource,
    profile::InputProfile,
    render::{Protocol, protocols},
    tui::TerminalSession,
};

const MIN_ZOOM: f64 = 0.1;
const MAX_ZOOM: f64 = 16.0;
const ZOOM_STEP: f64 = 1.25;

#[derive(Debug, Default, Clone, Copy)]
struct MouseDragState {
    button: Option<MouseButton>,
    last_column: i32,
    last_row: i32,
}

impl MouseDragState {
    fn start(&mut self, button: MouseButton, column: u16, row: u16) {
        self.button = Some(button);
        self.last_column = i32::from(column);
        self.last_row = i32::from(row);
    }

    fn apply(
        &mut self,
        button: MouseButton,
        column: u16,
        row: u16,
        state: &mut ImageState,
        cols: u32,
        rows: u32,
    ) {
        if self.button != Some(button) {
            return;
        }

        let next_column = i32::from(column);
        let next_row = i32::from(row);
        let delta_x = next_column.saturating_sub(self.last_column);
        let delta_y = next_row.saturating_sub(self.last_row);

        state.pan_by_delta(delta_x, delta_y, cols, rows);

        self.last_column = next_column;
        self.last_row = next_row;
    }

    fn end(&mut self, button: MouseButton) {
        if self.button == Some(button) {
            self.button = None;
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ImageState {
    fit: bool,
    zoom: f64,
    pan_x: i32,
    pan_y: i32,
    render_width: u32,
    render_height: u32,
}

impl ImageState {
    fn new() -> Self {
        Self {
            fit: true,
            zoom: 1.0,
            pan_x: 0,
            pan_y: 0,
            render_width: 1,
            render_height: 1,
        }
    }

    fn set_fit(&mut self, image_width: u32, image_height: u32, cols: u32, rows: u32) {
        self.fit = true;
        self.zoom = 1.0;
        let (width, height) = self.target_render_size(image_width, image_height, cols, rows);
        self.render_width = width;
        self.render_height = height;
        self.clamp_pan(cols, rows);
        self.pan_x = 0;
        self.pan_y = 0;
    }

    fn set_actual(&mut self, image_width: u32, image_height: u32, cols: u32, rows: u32) {
        self.fit = false;
        self.zoom = 1.0;
        let (width, height) = self.target_render_size(image_width, image_height, cols, rows);
        self.render_width = width;
        self.render_height = height;
        self.clamp_pan(cols, rows);
        self.pan_x = 0;
        self.pan_y = 0;
    }

    fn zoom_in(&mut self, image_width: u32, image_height: u32, cols: u32, rows: u32) {
        self.fit = false;
        self.zoom = (self.zoom * ZOOM_STEP).min(MAX_ZOOM);
        let (width, height) = self.target_render_size(image_width, image_height, cols, rows);
        self.render_width = width;
        self.render_height = height;
        self.clamp_pan(cols, rows);
    }

    fn zoom_out(&mut self, image_width: u32, image_height: u32, cols: u32, rows: u32) {
        self.fit = false;
        self.zoom = (self.zoom / ZOOM_STEP).max(MIN_ZOOM);
        let (width, height) = self.target_render_size(image_width, image_height, cols, rows);
        self.render_width = width;
        self.render_height = height;
        self.clamp_pan(cols, rows);
    }

    fn pan_left(&mut self, step: i32, cols: u32, rows: u32) {
        self.pan_x = self.pan_x.saturating_sub(step);
        self.clamp_pan(cols, rows);
    }

    fn pan_right(&mut self, step: i32, cols: u32, rows: u32) {
        self.pan_x = self.pan_x.saturating_add(step);
        self.clamp_pan(cols, rows);
    }

    fn pan_up(&mut self, step: i32, cols: u32, rows: u32) {
        self.pan_y = self.pan_y.saturating_sub(step);
        self.clamp_pan(cols, rows);
    }

    fn pan_down(&mut self, step: i32, cols: u32, rows: u32) {
        self.pan_y = self.pan_y.saturating_add(step);
        self.clamp_pan(cols, rows);
    }

    fn pan_by_delta(&mut self, delta_x: i32, delta_y: i32, cols: u32, rows: u32) {
        let candidate_pan_x = i64::from(self.pan_x) - i64::from(delta_x);
        let candidate_pan_y = i64::from(self.pan_y) - i64::from(delta_y);
        self.pan_x = clamp_i32_non_negative(candidate_pan_x);
        self.pan_y = clamp_i32_non_negative(candidate_pan_y);
        self.clamp_pan(cols, rows);
    }

    fn target_render_size(
        &self,
        image_width: u32,
        image_height: u32,
        cols: u32,
        rows: u32,
    ) -> (u32, u32) {
        if self.fit {
            protocols::blocks::fit_dimensions(image_width, image_height, max(1, cols), max(1, rows))
        } else {
            let zoom = self.zoom.clamp(MIN_ZOOM, MAX_ZOOM);
            let width = ((image_width as f64 * zoom).round()).max(1.0) as u32;
            let height = ((image_height as f64 * zoom).round()).max(1.0) as u32;
            (width.max(1), height.max(1))
        }
    }

    fn clamp_pan(&mut self, cols: u32, rows: u32) {
        let view_width = cols.max(1);
        let view_height = rows.max(1).saturating_mul(2).max(1);
        let max_pan_x = self.render_width.saturating_sub(view_width) as i32;
        let max_pan_y = self.render_height.saturating_sub(view_height) as i32;
        self.pan_x = self.pan_x.clamp(0, max_pan_x.max(0));
        self.pan_y = self.pan_y.clamp(0, max_pan_y.max(0));
    }
}

pub(crate) fn run(source: InputSource, _profile: InputProfile, protocol: Protocol) -> Result<()> {
    let image = load_image(&source).context("opening image for interactive view")?;
    let mut session = TerminalSession::start()?;

    let mut state = ImageState::new();
    let mut drag = MouseDragState::default();
    let mut show_metadata = false;
    let size = session.size().context("reading initial terminal size")?;
    state.set_fit(
        image.width(),
        image.height(),
        u32::from(size.width),
        u32::from(size.height.saturating_sub(1)),
    );

    let mut size = size;
    loop {
        let cols = u32::from(size.width);
        let rows = u32::from(size.height.saturating_sub(1));
        let frame = if show_metadata {
            metadata_overlay(source.label(), &image, &state, protocol, size.width)
        } else {
            render_frame(&image, cols.max(1), rows.max(1), protocol, &mut state)?
        };
        let status = status_line(&state, protocol, size.width, show_metadata);
        if show_metadata || protocol != Protocol::Blocks {
            session.draw_frame(&frame, &status)?;
        } else {
            session.draw_protocol_frame(&frame, &status)?;
        }

        match session.read_event()? {
            Some(Event::Key(key_event)) if key_event.kind == KeyEventKind::Press => {
                let pan_step_x = (cols / 8).max(4).max(1) as i32;
                let pan_step_y = (rows / 4).max(2).max(1) as i32;

                match key_event.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => break,
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        state.zoom_in(image.width(), image.height(), cols, rows);
                    }
                    KeyCode::Char('-') => {
                        state.zoom_out(image.width(), image.height(), cols, rows);
                    }
                    KeyCode::Char('0') => {
                        state.set_fit(image.width(), image.height(), cols, rows);
                    }
                    KeyCode::Char('1') => {
                        state.set_actual(image.width(), image.height(), cols, rows);
                    }
                    KeyCode::Char('m') | KeyCode::Char('M') => {
                        show_metadata = !show_metadata;
                    }
                    KeyCode::Left => state.pan_left(pan_step_x, cols, rows),
                    KeyCode::Right => state.pan_right(pan_step_x, cols, rows),
                    KeyCode::Up => state.pan_up(pan_step_y, cols, rows),
                    KeyCode::Down => state.pan_down(pan_step_y, cols, rows),
                    _ => {}
                }
            }
            Some(Event::Mouse(mouse_event)) => match mouse_event.kind {
                MouseEventKind::Down(button) if button == MouseButton::Left => {
                    drag.start(button, mouse_event.column, mouse_event.row);
                }
                MouseEventKind::Drag(button) if button == MouseButton::Left => {
                    let pan_cols = cols.max(1);
                    let pan_rows = rows.max(1);
                    drag.apply(
                        button,
                        mouse_event.column,
                        mouse_event.row,
                        &mut state,
                        pan_cols,
                        pan_rows,
                    );
                }
                MouseEventKind::Up(button) => {
                    drag.end(button);
                }
                _ => {}
            },
            Some(Event::Resize(cols, rows)) => {
                size.width = cols.max(1);
                size.height = rows.max(1);
                state.set_fit(
                    image.width(),
                    image.height(),
                    u32::from(size.width),
                    u32::from(size.height.saturating_sub(1)),
                );
                drag.button = None;
            }
            Some(_) | None => {}
        }
    }

    Ok(())
}

fn load_image(source: &InputSource) -> Result<DynamicImage> {
    ImageReader::open(source.path())
        .with_context(|| format!("opening {} for interactive image viewer", source.label()))?
        .with_guessed_format()
        .context("detecting image format")?
        .decode()
        .context("decoding image")
}

fn render_frame(
    image: &DynamicImage,
    cols: u32,
    rows: u32,
    protocol: Protocol,
    state: &mut ImageState,
) -> Result<String> {
    let (render_width, render_height) =
        state.target_render_size(image.width(), image.height(), cols, rows);
    if render_width == 0 || render_height == 0 {
        return Ok(String::new());
    }
    state.render_width = render_width;
    state.render_height = render_height;
    state.clamp_pan(cols, rows);

    let resized = image.resize_exact(render_width, render_height, FilterType::Nearest);
    let crop_width = cols.min(resized.width());
    let crop_height = (rows * 2).min(resized.height());
    let pan_x = state
        .pan_x
        .clamp(0, resized.width().saturating_sub(crop_width).max(1) as i32);
    let pan_y = state.pan_y.clamp(
        0,
        resized.height().saturating_sub(crop_height).max(1) as i32,
    );
    state.pan_x = pan_x;
    state.pan_y = pan_y;

    let cropped = resized.crop_imm(
        u32::try_from(pan_x).unwrap_or(0),
        u32::try_from(pan_y).unwrap_or(0),
        crop_width,
        crop_height,
    );
    match protocol {
        Protocol::Blocks => {
            protocols::blocks::render_raster_for_size(&cropped, cols.max(1), rows.max(1))
        }
        Protocol::Kitty => protocols::kitty::render(&cropped),
        Protocol::Sixel => protocols::sixel::render(&cropped),
        Protocol::Iterm => protocols::iterm::render(&cropped),
        Protocol::Auto => unreachable!("auto protocol should be resolved before rendering"),
    }
}

fn status_line(state: &ImageState, protocol: Protocol, width: u16, show_metadata: bool) -> String {
    let zoom_label = if state.fit {
        "fit".to_owned()
    } else {
        format!("{:.2}x", state.zoom)
    };
    let protocol_text = crate::render::protocols::protocol_name(protocol);
    let mode = if show_metadata {
        "m image"
    } else {
        "m metadata"
    };
    let status = format!(
        "{protocol_text} | {zoom_label} | pan {},{} | {mode} | q quit",
        state.pan_x, state.pan_y
    );
    trim_to_width(&status, usize::from(width))
}

fn metadata_overlay(
    label: &str,
    image: &DynamicImage,
    state: &ImageState,
    protocol: Protocol,
    width: u16,
) -> String {
    let zoom_label = if state.fit {
        "fit".to_owned()
    } else {
        format!("{:.2}x", state.zoom)
    };
    let protocol_text = crate::render::protocols::protocol_name(protocol);
    let lines = vec![
        format!("file: {label}"),
        format!("image: {}x{}", image.width(), image.height()),
        format!("render: {}x{}", state.render_width, state.render_height),
        format!("zoom: {zoom_label}"),
        format!("pan: {},{}", state.pan_x, state.pan_y),
        format!("protocol: {protocol_text}"),
        "controls: q quit | m image".to_owned(),
    ];
    let target_width = usize::from(width.max(1));
    let mut output = String::new();
    for line in lines {
        output.push_str(&trim_to_width(&line, target_width));
        output.push('\n');
    }
    output
}

fn clamp_i32_non_negative(value: i64) -> i32 {
    let clamped = value.clamp(0, i64::from(i32::MAX));
    i32::try_from(clamped).unwrap_or(0)
}

fn trim_to_width(text: &str, width: usize) -> String {
    let mut output = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(1);
        if used + ch_width > width {
            break;
        }
        used += ch_width.max(1);
        output.push(ch);
    }
    if used < width {
        output.push_str(&" ".repeat(width - used));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pan_by_delta_moves_viewport_in_cell_units() {
        let mut state = ImageState::new();
        state.render_width = 80;
        state.render_height = 40;
        state.pan_x = 20;
        state.pan_y = 10;

        state.pan_by_delta(4, 6, 20, 10);

        assert_eq!(state.pan_x, 16);
        assert_eq!(state.pan_y, 4);
    }

    #[test]
    fn drag_state_updates_position_only_for_left_drag() {
        let mut state = ImageState::new();
        state.render_width = 80;
        state.render_height = 40;
        let mut drag = MouseDragState::default();

        drag.start(MouseButton::Left, 5, 5);
        drag.apply(MouseButton::Left, 10, 7, &mut state, 20, 10);

        assert_eq!(state.pan_x, 0);
        assert_eq!(state.pan_y, 0);
        assert_eq!(drag.button, Some(MouseButton::Left));
        assert_eq!(drag.last_column, 10);
        assert_eq!(drag.last_row, 7);

        drag.apply(MouseButton::Right, 20, 12, &mut state, 20, 10);
        assert_eq!(state.pan_x, 0);
        assert_eq!(state.pan_y, 0);

        drag.end(MouseButton::Left);
        drag.apply(MouseButton::Left, 15, 15, &mut state, 20, 10);
        assert_eq!(state.pan_x, 0);
        assert_eq!(state.pan_y, 0);
    }
}
