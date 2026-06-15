use std::cmp::max;

use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use image::{DynamicImage, ImageReader, imageops::FilterType};
use unicode_width::UnicodeWidthChar;

use crate::{
    input::InputSource, profile::InputProfile, render::Protocol, render::protocols::blocks,
    tui::TerminalSession,
};

const MIN_ZOOM: f64 = 0.1;
const MAX_ZOOM: f64 = 16.0;
const ZOOM_STEP: f64 = 1.25;

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

    fn target_render_size(
        &self,
        image_width: u32,
        image_height: u32,
        cols: u32,
        rows: u32,
    ) -> (u32, u32) {
        if self.fit {
            blocks::fit_dimensions(image_width, image_height, max(1, cols), max(1, rows))
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

pub(crate) fn run(
    source: InputSource,
    _profile: InputProfile,
    _protocol: Protocol,
    protocol_note: Option<&str>,
) -> Result<()> {
    let image = load_image(&source).context("opening image for interactive view")?;
    let mut session = TerminalSession::start()?;

    let mut state = ImageState::new();
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
        let frame = render_frame(&image, cols.max(1), rows.max(1), &mut state)?;
        let status = status_line(&state, protocol_note, size.width);
        session.draw_frame(&frame, &status)?;

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
                    KeyCode::Left => state.pan_left(pan_step_x, cols, rows),
                    KeyCode::Right => state.pan_right(pan_step_x, cols, rows),
                    KeyCode::Up => state.pan_up(pan_step_y, cols, rows),
                    KeyCode::Down => state.pan_down(pan_step_y, cols, rows),
                    _ => {}
                }
            }
            Some(Event::Resize(cols, rows)) => {
                size.width = cols.max(1);
                size.height = rows.max(1);
                state.set_fit(
                    image.width(),
                    image.height(),
                    u32::from(size.width),
                    u32::from(size.height.saturating_sub(1)),
                );
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
    blocks::render_raster_for_size(&cropped, cols.max(1), rows.max(1))
}

fn status_line(state: &ImageState, protocol_note: Option<&str>, width: u16) -> String {
    let zoom_label = if state.fit {
        "fit".to_owned()
    } else {
        format!("{:.2}x", state.zoom)
    };
    let protocol_text = protocol_note.unwrap_or("protocol: blocks");
    let status = format!(
        "{protocol_text} | {zoom_label} | pan {},{} | q quit",
        state.pan_x, state.pan_y
    );
    trim_to_width(&status, usize::from(width))
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
