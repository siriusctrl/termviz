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
    let image = load_image_rgba(&source).context("opening image for interactive view")?;
    let mut session = TerminalSession::start()?;

    let mut state = ImageState::new();
    let mut frame_cache = ImageFrameCache::default();
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
                render_frame(
                    &image,
                    cols.max(1),
                    rows.max(1),
                    protocol,
                    &mut state,
                    &mut frame_cache,
                )?
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

fn load_image_rgba(source: &InputSource) -> Result<RgbaImage> {
    Ok(load_image(source)?.to_rgba8())
}

fn render_frame(
    image: &RgbaImage,
    cols: u32,
    rows: u32,
    protocol: Protocol,
    state: &mut ImageState,
    cache: &mut ImageFrameCache,
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

    let resized = cache.resized_image(image, render_width, render_height);
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

    let frame_image = compose_view_on_dark_canvas(resized, canvas_size, pan_x, pan_y);
    let context = ProtocolRenderContext::new(protocol);
    Ok(protocols::render_raster_rgba_with_fallback(
        context,
        &frame_image,
        cols.max(1),
        rows.max(1),
    ))
}

#[derive(Debug, Default)]
struct ImageFrameCache {
    resized_key: Option<(u32, u32)>,
    resized: Option<RgbaImage>,
}

impl ImageFrameCache {
    fn resized_image(&mut self, image: &RgbaImage, width: u32, height: u32) -> &RgbaImage {
        let key = (width.max(1), height.max(1));
        if self.resized_key != Some(key) {
            self.resized = Some(imageops::resize(
                image,
                key.0,
                key.1,
                FilterType::CatmullRom,
            ));
            self.resized_key = Some(key);
        }
        self.resized.as_ref().expect("resized image is cached")
    }
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
    image: &RgbaImage,
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

fn compose_view_on_dark_canvas(
    image: &RgbaImage,
    canvas_size: (u32, u32),
    pan_x: i32,
    pan_y: i32,
) -> RgbaImage {
    let (canvas_width, canvas_height) = canvas_size;
    let canvas_width = canvas_width.max(1);
    let canvas_height = canvas_height.max(1);
    let paste_x = if image.width() < canvas_width {
        i64::from(canvas_width.saturating_sub(image.width()).saturating_div(2))
    } else {
        -i64::from(pan_x.max(0))
    };
    let paste_y = if image.height() < canvas_height {
        i64::from(
            canvas_height
                .saturating_sub(image.height())
                .saturating_div(2),
        )
    } else {
        -i64::from(pan_y.max(0))
    };
    let mut canvas = RgbaImage::from_pixel(canvas_width, canvas_height, DARK_MATTE);
    overlay_on_dark_canvas(&mut canvas, image, paste_x, paste_y);
    canvas
}

fn overlay_on_dark_canvas(canvas: &mut RgbaImage, image: &RgbaImage, paste_x: i64, paste_y: i64) {
    let Some(bounds) = copy_bounds(
        (canvas.width(), canvas.height()),
        (image.width(), image.height()),
        paste_x,
        paste_y,
    ) else {
        return;
    };

    let src = image.as_raw();
    let dst_width = canvas.width();
    let dst: &mut [u8] = canvas.as_mut();
    let matte = DARK_MATTE.0;
    for row in 0..bounds.height {
        let src_row = u64::from(bounds.src_y + row) * u64::from(image.width());
        let dst_row = u64::from(bounds.dst_y + row) * u64::from(dst_width);
        for col in 0..bounds.width {
            let src_offset = ((src_row + u64::from(bounds.src_x + col)) * 4) as usize;
            let dst_offset = ((dst_row + u64::from(bounds.dst_x + col)) * 4) as usize;
            let alpha = src[src_offset + 3];
            if alpha == 0 {
                continue;
            }
            if alpha == 255 {
                dst[dst_offset..dst_offset + 4].copy_from_slice(&src[src_offset..src_offset + 4]);
                continue;
            }

            let inverse = u16::from(255 - alpha);
            let alpha_u16 = u16::from(alpha);
            for channel in 0..3 {
                dst[dst_offset + channel] = ((u16::from(src[src_offset + channel]) * alpha_u16
                    + u16::from(matte[channel]) * inverse)
                    / 255) as u8;
            }

            let max = 255.0f32;
            let fg_a = f32::from(alpha) / max;
            let alpha_final = 1.0 + fg_a - fg_a;
            dst[dst_offset + 3] = (max * alpha_final) as u8;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CopyBounds {
    dst_x: u32,
    dst_y: u32,
    src_x: u32,
    src_y: u32,
    width: u32,
    height: u32,
}

fn copy_bounds(
    canvas_size: (u32, u32),
    image_size: (u32, u32),
    paste_x: i64,
    paste_y: i64,
) -> Option<CopyBounds> {
    let (dst_width, dst_height) = (canvas_size.0.max(1), canvas_size.1.max(1));
    let (src_width, src_height) = image_size;
    let dst_x = paste_x.max(0) as u32;
    let dst_y = paste_y.max(0) as u32;
    let src_x = paste_x.saturating_neg().max(0) as u32;
    let src_y = paste_y.saturating_neg().max(0) as u32;
    let width = src_width
        .saturating_sub(src_x)
        .min(dst_width.saturating_sub(dst_x));
    let height = src_height
        .saturating_sub(src_y)
        .min(dst_height.saturating_sub(dst_y));
    (width > 0 && height > 0).then_some(CopyBounds {
        dst_x,
        dst_y,
        src_x,
        src_y,
        width,
        height,
    })
}

#[cfg(test)]
fn compose_on_dark_canvas(image: &RgbaImage, canvas_size: (u32, u32)) -> RgbaImage {
    compose_view_on_dark_canvas(image, canvas_size, 0, 0)
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
    use crate::{input::InputSource, profile::InputProfile};
    use image::{ImageBuffer, Rgba};
    use std::{hint::black_box, time::Duration, time::Instant};

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
        let image = ImageBuffer::from_pixel(1, 1, Rgba([255, 255, 255, 255]));
        let mut state = ImageState::new();
        let mut cache = ImageFrameCache::default();

        let frame = render_frame(&image, 80, 24, Protocol::Kitty, &mut state, &mut cache).unwrap();

        assert!(frame.contains("\x1b_G"));
        assert!(frame.contains(",c=80,r=24,"));
    }

    #[test]
    fn image_frame_renders_every_explicit_protocol() {
        let image = ImageBuffer::from_pixel(2, 2, Rgba([255, 255, 255, 128]));
        let cases = [
            (Protocol::Blocks, "\x1b[38;2;"),
            (Protocol::Kitty, "\x1b_G"),
        ];

        for (protocol, marker) in cases {
            let mut state = ImageState::new();
            let mut cache = ImageFrameCache::default();
            let frame = render_frame(&image, 24, 12, protocol, &mut state, &mut cache)
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
        let image = ImageBuffer::from_pixel(1, 1, Rgba([255, 255, 255, 255]));
        let mut state = ImageState::new();
        let mut cache = ImageFrameCache::default();

        let _frame = render_frame(&image, 80, 24, Protocol::Kitty, &mut state, &mut cache).unwrap();

        assert_eq!(state.render_width, 384);
        assert_eq!(state.render_height, 384);
    }

    #[test]
    fn transparent_images_are_composited_on_dark_matte() {
        let image = ImageBuffer::from_pixel(2, 2, Rgba([255, 255, 255, 0]));

        let composed = compose_on_dark_canvas(&image, (4, 4));

        assert_eq!(composed.get_pixel(0, 0), &DARK_MATTE);
        assert_eq!(composed.get_pixel(2, 2), &DARK_MATTE);
    }

    #[test]
    fn dark_canvas_composition_matches_overlay_alpha_blending() {
        let image = ImageBuffer::from_fn(16, 16, |x, y| {
            let alpha = (x * 16 + y) as u8;
            Rgba([
                (13 + x * 11) as u8,
                (29 + y * 7) as u8,
                (47 + (x + y) * 5) as u8,
                alpha,
            ])
        });

        let mut expected = RgbaImage::from_pixel(6, 5, DARK_MATTE);
        imageops::overlay(&mut expected, &image, -2, 1);

        let mut actual = RgbaImage::from_pixel(6, 5, DARK_MATTE);
        overlay_on_dark_canvas(&mut actual, &image, -2, 1);

        assert_eq!(actual.as_raw(), expected.as_raw());
    }

    #[test]
    fn image_frame_cache_reuses_resized_image_for_same_target() {
        let image = ImageBuffer::from_pixel(4, 3, Rgba([255, 255, 255, 255]));
        let mut cache = ImageFrameCache::default();

        let first_ptr = cache.resized_image(&image, 8, 6).as_raw().as_ptr();
        let second_ptr = cache.resized_image(&image, 8, 6).as_raw().as_ptr();
        let third_ptr = cache.resized_image(&image, 9, 6).as_raw().as_ptr();

        assert_eq!(first_ptr, second_ptr);
        assert_ne!(second_ptr, third_ptr);
    }

    #[test]
    #[ignore = "local perf test; run scripts/bench-render-pipeline.sh"]
    fn image_render_pipeline_perf() {
        let iterations = std::env::var("TERMVIZ_RENDER_PIPELINE_ITERS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(12);
        let file = dense_perf_png();

        print_image_perf_metric("kitty_image_full_pipeline_120x32", iterations, || {
            profile_image_startup_and_frame(file.path(), Protocol::Kitty, 120, 31)
                .expect("profile full kitty image pipeline")
        });
        print_image_perf_metric("blocks_image_full_pipeline_120x32", iterations, || {
            profile_image_startup_and_frame(file.path(), Protocol::Blocks, 120, 31)
                .expect("profile full blocks image pipeline")
        });

        let source = InputSource::from_path(file.path().to_path_buf()).unwrap();
        let image = load_image_rgba(&source).unwrap();
        print_image_perf_metric("kitty_image_hot_pan_120x32", iterations, || {
            let mut state = ImageState::new();
            state.set_actual(
                image.width(),
                image.height(),
                120 * CELL_PIXEL_WIDTH,
                31 * CELL_PIXEL_HEIGHT,
            );
            let mut cache = ImageFrameCache::default();
            profile_image_frame(&image, 120, 31, Protocol::Kitty, &mut state, &mut cache)
                .expect("warm image cache");
            state.pan_right(64, 120 * CELL_PIXEL_WIDTH, 31 * CELL_PIXEL_HEIGHT);
            profile_image_frame(&image, 120, 31, Protocol::Kitty, &mut state, &mut cache)
                .expect("profile hot kitty image pan")
        });
    }

    #[derive(Debug, Clone)]
    struct ImagePerfSample {
        total: Duration,
        profile: Duration,
        load: Duration,
        layout: Duration,
        raster: Duration,
        compose: Duration,
        protocol: Duration,
        bytes: usize,
        image_pixels: u64,
    }

    fn profile_image_startup_and_frame(
        path: &std::path::Path,
        protocol: Protocol,
        cols: u32,
        rows: u32,
    ) -> Result<ImagePerfSample> {
        let total_start = Instant::now();

        let profile_start = Instant::now();
        let source = InputSource::from_path(path.to_path_buf())?;
        let _profile = InputProfile::resolve(&source, crate::plot::PlotKind::Line, None)?;
        let profile_time = profile_start.elapsed();

        let load_start = Instant::now();
        let image = load_image_rgba(&source)?;
        let load_time = load_start.elapsed();

        let mut state = ImageState::new();
        let mut cache = ImageFrameCache::default();
        let mut sample = profile_image_frame(&image, cols, rows, protocol, &mut state, &mut cache)?;
        sample.profile = profile_time;
        sample.load = load_time;
        sample.total = total_start.elapsed();
        Ok(sample)
    }

    fn profile_image_frame(
        image: &RgbaImage,
        cols: u32,
        rows: u32,
        protocol: Protocol,
        state: &mut ImageState,
        cache: &mut ImageFrameCache,
    ) -> Result<ImagePerfSample> {
        let total_start = Instant::now();

        let layout_start = Instant::now();
        let canvas_size = render_canvas_size(protocol, cols.max(1), rows.max(1));
        let (render_width, render_height) =
            state.target_render_size(image.width(), image.height(), canvas_size.0, canvas_size.1);
        state.render_width = render_width;
        state.render_height = render_height;
        state.clamp_pan(canvas_size.0, canvas_size.1);
        let layout_time = layout_start.elapsed();

        let resize_start = Instant::now();
        let resized = cache.resized_image(image, render_width, render_height);
        let resize_time = resize_start.elapsed();

        let compose_start = Instant::now();
        let crop_width = canvas_size.0.min(resized.width());
        let crop_height = canvas_size.1.min(resized.height());
        let pan_x = state
            .pan_x
            .clamp(0, resized.width().saturating_sub(crop_width).max(1) as i32);
        let pan_y = state.pan_y.clamp(
            0,
            resized.height().saturating_sub(crop_height).max(1) as i32,
        );
        let frame_image = compose_view_on_dark_canvas(resized, canvas_size, pan_x, pan_y);
        let image_pixels = u64::from(frame_image.width()) * u64::from(frame_image.height());
        let compose_time = compose_start.elapsed();

        let protocol_start = Instant::now();
        let payload = protocols::render_raster_rgba_with_fallback(
            ProtocolRenderContext::new(protocol),
            &frame_image,
            cols.max(1),
            rows.max(1),
        );
        let protocol_time = protocol_start.elapsed();

        Ok(ImagePerfSample {
            total: total_start.elapsed(),
            profile: Duration::ZERO,
            load: Duration::ZERO,
            layout: layout_time,
            raster: resize_time,
            compose: compose_time,
            protocol: protocol_time,
            bytes: payload.len(),
            image_pixels,
        })
    }

    fn print_image_perf_metric<F>(name: &str, iterations: usize, mut run: F)
    where
        F: FnMut() -> ImagePerfSample,
    {
        let mut total_time = Duration::ZERO;
        let mut profile_time = Duration::ZERO;
        let mut load_time = Duration::ZERO;
        let mut layout_time = Duration::ZERO;
        let mut raster_time = Duration::ZERO;
        let mut compose_time = Duration::ZERO;
        let mut protocol_time = Duration::ZERO;
        let mut bytes = 0usize;
        let mut image_pixels = 0u64;

        for _ in 0..iterations {
            let sample = run();
            total_time += sample.total;
            profile_time += sample.profile;
            load_time += sample.load;
            layout_time += sample.layout;
            raster_time += sample.raster;
            compose_time += sample.compose;
            protocol_time += sample.protocol;
            bytes = bytes.saturating_add(black_box(sample.bytes));
            image_pixels = image_pixels.saturating_add(black_box(sample.image_pixels));
        }

        let total_us = total_time.as_micros();
        let mean_total_us = total_us / iterations as u128;
        let mean_profile_us = profile_time.as_micros() / iterations as u128;
        let mean_load_us = load_time.as_micros() / iterations as u128;
        let mean_layout_us = layout_time.as_micros() / iterations as u128;
        let mean_raster_us = raster_time.as_micros() / iterations as u128;
        let mean_compose_us = compose_time.as_micros() / iterations as u128;
        let mean_protocol_us = protocol_time.as_micros() / iterations as u128;
        let mean_bytes = bytes / iterations;
        let mean_image_pixels = image_pixels / iterations as u64;

        println!(
            "image_pipeline_detail,{name},{iterations},{total_us},{mean_total_us},{mean_profile_us},{mean_load_us},{mean_layout_us},0,{mean_raster_us},{mean_compose_us},{mean_protocol_us},0,{bytes},{mean_bytes},0,0,{mean_image_pixels}"
        );
    }

    fn dense_perf_png() -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::with_suffix(".png").unwrap();
        let mut image = RgbaImage::new(960, 540);
        for y in 0..image.height() {
            for x in 0..image.width() {
                let red = ((x * 255) / image.width()) as u8;
                let green = ((y * 255) / image.height()) as u8;
                let blue = (((x + y) * 255) / (image.width() + image.height())) as u8;
                image.put_pixel(x, y, Rgba([red, green, blue, 220]));
            }
        }
        DynamicImage::ImageRgba8(image)
            .save_with_format(file.path(), image::ImageFormat::Png)
            .unwrap();
        file
    }
}
