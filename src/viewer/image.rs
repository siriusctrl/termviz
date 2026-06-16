use std::cmp::max;

use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use image::{
    DynamicImage, ImageReader, Rgba, RgbaImage,
    imageops::{self, FilterType},
};
use unicode_width::UnicodeWidthChar;

use crate::{
    input::InputSource,
    profile::InputProfile,
    render::{Protocol, protocols, protocols::ProtocolRenderContext},
    tui::TerminalSession,
};

const MIN_ZOOM: f64 = 0.1;
const MAX_ZOOM: f64 = 16.0;
const ZOOM_STEP: f64 = 1.25;
const CELL_PIXEL_WIDTH: u32 = 8;
const CELL_PIXEL_HEIGHT: u32 = 16;
const DARK_MATTE: Rgba<u8> = Rgba([13, 17, 23, 255]);

#[derive(Debug, Default, Clone, Copy)]
struct MouseDragState {
    button: Option<MouseButton>,
    last_column: i32,
    last_row: i32,
}

#[derive(Debug, Clone, Copy)]
struct DragViewMetrics {
    view_width: u32,
    view_height: u32,
    terminal_cols: u32,
    terminal_rows: u32,
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
        metrics: DragViewMetrics,
    ) {
        if self.button != Some(button) {
            return;
        }

        let next_column = i32::from(column);
        let next_row = i32::from(row);
        let delta_x = scale_cell_delta(
            next_column.saturating_sub(self.last_column),
            metrics.view_width,
            metrics.terminal_cols,
        );
        let delta_y = scale_cell_delta(
            next_row.saturating_sub(self.last_row),
            metrics.view_height,
            metrics.terminal_rows,
        );

        state.pan_by_delta(delta_x, delta_y, metrics.view_width, metrics.view_height);

        self.last_column = next_column;
        self.last_row = next_row;
    }

    fn end(&mut self, button: MouseButton) {
        if self.button == Some(button) {
            self.button = None;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
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
            fit_dimensions_for_view(image_width, image_height, max(1, cols), max(1, rows))
        } else {
            let zoom = self.zoom.clamp(MIN_ZOOM, MAX_ZOOM);
            let width = ((image_width as f64 * zoom).round()).max(1.0) as u32;
            let height = ((image_height as f64 * zoom).round()).max(1.0) as u32;
            (width.max(1), height.max(1))
        }
    }

    fn clamp_pan(&mut self, cols: u32, rows: u32) {
        let view_width = cols.max(1);
        let view_height = rows.max(1);
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
    let initial_canvas = render_canvas_size(
        protocol,
        u32::from(size.width),
        u32::from(size.height.saturating_sub(1)).max(1),
    );
    state.set_fit(
        image.width(),
        image.height(),
        initial_canvas.0,
        initial_canvas.1,
    );

    let mut size = size;
    let mut dirty = true;
    loop {
        let cols = u32::from(size.width);
        let rows = u32::from(size.height.saturating_sub(1));
        let canvas = render_canvas_size(protocol, cols.max(1), rows.max(1));
        if dirty {
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
            dirty = false;
        }

        match session.read_event()? {
            Some(Event::Key(key_event)) if key_event.kind == KeyEventKind::Press => {
                let pan_step_x = (canvas.0 / 8).max(4).max(1) as i32;
                let pan_step_y = (canvas.1 / 8).max(4).max(1) as i32;
                let previous_state = state;
                let previous_metadata = show_metadata;

                match key_event.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => break,
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        state.zoom_in(image.width(), image.height(), canvas.0, canvas.1);
                    }
                    KeyCode::Char('-') => {
                        state.zoom_out(image.width(), image.height(), canvas.0, canvas.1);
                    }
                    KeyCode::Char('0') => {
                        state.set_fit(image.width(), image.height(), canvas.0, canvas.1);
                    }
                    KeyCode::Char('1') => {
                        state.set_actual(image.width(), image.height(), canvas.0, canvas.1);
                    }
                    KeyCode::Char('m') | KeyCode::Char('M') => {
                        show_metadata = !show_metadata;
                    }
                    KeyCode::Left => state.pan_left(pan_step_x, canvas.0, canvas.1),
                    KeyCode::Right => state.pan_right(pan_step_x, canvas.0, canvas.1),
                    KeyCode::Up => state.pan_up(pan_step_y, canvas.0, canvas.1),
                    KeyCode::Down => state.pan_down(pan_step_y, canvas.0, canvas.1),
                    _ => {}
                }
                dirty = dirty || state != previous_state || show_metadata != previous_metadata;
            }
            Some(Event::Mouse(mouse_event)) => match mouse_event.kind {
                MouseEventKind::Down(button) if button == MouseButton::Left => {
                    drag.start(button, mouse_event.column, mouse_event.row);
                }
                MouseEventKind::Drag(button) if button == MouseButton::Left => {
                    let previous_state = state;
                    drag.apply(
                        button,
                        mouse_event.column,
                        mouse_event.row,
                        &mut state,
                        DragViewMetrics {
                            view_width: canvas.0,
                            view_height: canvas.1,
                            terminal_cols: cols.max(1),
                            terminal_rows: rows.max(1),
                        },
                    );
                    dirty = dirty || state != previous_state;
                }
                MouseEventKind::Up(button) => {
                    drag.end(button);
                }
                _ => {}
            },
            Some(Event::Resize(cols, rows)) => {
                let previous_size = size;
                let previous_state = state;
                size.width = cols.max(1);
                size.height = rows.max(1);
                let resized_canvas = render_canvas_size(
                    protocol,
                    u32::from(size.width),
                    u32::from(size.height.saturating_sub(1)).max(1),
                );
                state.set_fit(
                    image.width(),
                    image.height(),
                    resized_canvas.0,
                    resized_canvas.1,
                );
                drag.button = None;
                dirty = dirty || size != previous_size || state != previous_state;
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
    let canvas_size = render_canvas_size(protocol, cols.max(1), rows.max(1));
    let (render_width, render_height) =
        state.target_render_size(image.width(), image.height(), canvas_size.0, canvas_size.1);
    if render_width == 0 || render_height == 0 {
        return Ok(String::new());
    }
    state.render_width = render_width;
    state.render_height = render_height;
    state.clamp_pan(canvas_size.0, canvas_size.1);

    let resized = image.resize_exact(render_width, render_height, FilterType::CatmullRom);
    let crop_width = canvas_size.0.min(resized.width());
    let crop_height = canvas_size.1.min(resized.height());
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
    let frame_image = compose_on_dark_canvas(&cropped, canvas_size);
    let context = ProtocolRenderContext::new(protocol);
    Ok(protocols::render_raster_with_fallback(
        context,
        &frame_image,
        cols.max(1),
        rows.max(1),
    ))
}

fn status_line(state: &ImageState, protocol: Protocol, width: u16, show_metadata: bool) -> String {
    let zoom_label = if state.fit {
        "fit".to_owned()
    } else {
        format!("{:.2}x", state.zoom)
    };
    let mode = if show_metadata {
        "m image"
    } else {
        "m metadata"
    };
    let status = format!(
        "{} · {zoom_label} · pan {},{} · +/- zoom · 0 fit · {mode} · q quit",
        protocol_label(protocol),
        state.pan_x,
        state.pan_y
    );
    trim_to_width(&status, usize::from(width))
}

fn protocol_label(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Auto => "auto",
        Protocol::Kitty => "kitty",
        Protocol::Blocks => "blocks",
    }
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

fn fit_dimensions_for_view(
    source_width: u32,
    source_height: u32,
    max_columns: u32,
    max_rows: u32,
) -> (u32, u32) {
    if source_width == 0 || source_height == 0 {
        return (0, 0);
    }

    let max_pixel_rows = max_rows.max(1);
    let width_scale = max_columns as f64 / source_width as f64;
    let height_scale = max_pixel_rows as f64 / source_height as f64;
    let scale = width_scale.min(height_scale).max(0.01);
    let scaled_width = ((source_width as f64 * scale).round()).max(1.0) as u32;
    let scaled_height = ((source_height as f64 * scale).round()).max(1.0) as u32;
    (scaled_width, scaled_height)
}

fn render_canvas_size(protocol: Protocol, cols: u32, rows: u32) -> (u32, u32) {
    let cols = cols.max(1);
    let rows = rows.max(1);
    match protocol {
        Protocol::Blocks => (cols, rows.saturating_mul(2).max(1)),
        Protocol::Kitty => (
            cols.saturating_mul(CELL_PIXEL_WIDTH).max(1),
            rows.saturating_mul(CELL_PIXEL_HEIGHT).max(1),
        ),
        Protocol::Auto => unreachable!("auto protocol should be resolved before rendering"),
    }
}

fn compose_on_dark_canvas(image: &DynamicImage, canvas_size: (u32, u32)) -> DynamicImage {
    let (canvas_width, canvas_height) = canvas_size;
    let mut canvas = RgbaImage::from_pixel(canvas_width.max(1), canvas_height.max(1), DARK_MATTE);
    let paste_x = canvas
        .width()
        .saturating_sub(image.width())
        .saturating_div(2);
    let paste_y = canvas
        .height()
        .saturating_sub(image.height())
        .saturating_div(2);
    imageops::overlay(
        &mut canvas,
        &image.to_rgba8(),
        paste_x.into(),
        paste_y.into(),
    );
    DynamicImage::ImageRgba8(canvas)
}

fn scale_cell_delta(delta: i32, view_size: u32, terminal_size: u32) -> i32 {
    let scale = view_size.max(1) as f64 / terminal_size.max(1) as f64;
    ((delta as f64) * scale).round() as i32
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
    use image::{ImageBuffer, Rgba};

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
        let metrics = DragViewMetrics {
            view_width: 20,
            view_height: 10,
            terminal_cols: 20,
            terminal_rows: 10,
        };

        drag.apply(MouseButton::Left, 10, 7, &mut state, metrics);

        assert_eq!(state.pan_x, 0);
        assert_eq!(state.pan_y, 0);
        assert_eq!(drag.button, Some(MouseButton::Left));
        assert_eq!(drag.last_column, 10);
        assert_eq!(drag.last_row, 7);

        drag.apply(MouseButton::Right, 20, 12, &mut state, metrics);
        assert_eq!(state.pan_x, 0);
        assert_eq!(state.pan_y, 0);

        drag.end(MouseButton::Left);
        drag.apply(MouseButton::Left, 15, 15, &mut state, metrics);
        assert_eq!(state.pan_x, 0);
        assert_eq!(state.pan_y, 0);
    }

    #[test]
    fn fit_mode_can_upscale_small_images_to_view() {
        let state = ImageState::new();
        let canvas = render_canvas_size(Protocol::Blocks, 80, 24);

        let (width, height) = state.target_render_size(1, 1, canvas.0, canvas.1);

        assert_eq!((width, height), (48, 48));
    }

    #[test]
    fn kitty_image_frame_requests_terminal_cell_size() {
        let image =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(1, 1, Rgba([255, 255, 255, 255])));
        let mut state = ImageState::new();

        let frame = render_frame(&image, 80, 24, Protocol::Kitty, &mut state).unwrap();

        assert!(frame.contains("\x1b_G"));
        assert!(frame.contains(",c=80,r=24,"));
    }

    #[test]
    fn image_frame_renders_every_explicit_protocol() {
        let image =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 2, Rgba([255, 255, 255, 128])));
        let cases = [
            (Protocol::Blocks, "\x1b[38;2;"),
            (Protocol::Kitty, "\x1b_G"),
        ];

        for (protocol, marker) in cases {
            let mut state = ImageState::new();
            let frame = render_frame(&image, 24, 12, protocol, &mut state)
                .unwrap_or_else(|error| panic!("{protocol:?} image frame failed: {error}"));

            assert!(
                frame.contains(marker),
                "{protocol:?} image frame did not include marker {marker:?}"
            );
            assert!(state.render_width > 0);
            assert!(state.render_height > 0);
        }
    }

    #[test]
    fn image_protocol_fit_uses_cell_aspect_canvas_without_stretching_source() {
        let image =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(1, 1, Rgba([255, 255, 255, 255])));
        let mut state = ImageState::new();

        let _frame = render_frame(&image, 80, 24, Protocol::Kitty, &mut state).unwrap();

        assert_eq!(state.render_width, 384);
        assert_eq!(state.render_height, 384);
    }

    #[test]
    fn transparent_images_are_composited_on_dark_matte() {
        let image =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 2, Rgba([255, 255, 255, 0])));

        let composed = compose_on_dark_canvas(&image, (4, 4)).to_rgba8();

        assert_eq!(composed.get_pixel(0, 0), &DARK_MATTE);
        assert_eq!(composed.get_pixel(2, 2), &DARK_MATTE);
    }
}
