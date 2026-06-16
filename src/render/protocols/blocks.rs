use crate::plot::{
    PlotKind,
    model::{PlotBounds, PlotPoint, PlotScene},
};
use anyhow::{Context, Result};
use image::{DynamicImage, Rgba};

const MAX_ANSI_COLUMNS: u32 = 80;
const MAX_ANSI_ROWS: u32 = 40;
const PLOT_WIDTH: u32 = 80;
const PLOT_HEIGHT: u32 = 30;
const SERIE_GLYPHS: &[char] = &['●', '○', '◉', '◆', '▲', '■'];
const TERMINAL_SERIE_GLYPHS: &[char] = &['*', 'o', '@', '#', '^', 's'];
const TERMINAL_BG: (u8, u8, u8) = (13, 17, 23);
const TERMINAL_TEXT: (u8, u8, u8) = (203, 213, 225);
const TERMINAL_AXIS: (u8, u8, u8) = (148, 163, 184);
const TERMINAL_SERIES: &[(u8, u8, u8)] = &[
    (96, 165, 250),
    (251, 146, 60),
    (52, 211, 153),
    (248, 113, 113),
    (196, 181, 253),
    (244, 114, 182),
];
const BRAILLE_LINE_RADIUS: f64 = 0.62;
const BRAILLE_JOIN_RADIUS: f64 = 0.9;

#[derive(Debug, Clone, Copy)]
struct PlotArea {
    left: usize,
    right: usize,
    top: usize,
    bottom: usize,
    width: f64,
    height: f64,
}

pub(crate) fn render_raster(image: &DynamicImage) -> Result<String> {
    render_raster_for_size(image, MAX_ANSI_COLUMNS, MAX_ANSI_ROWS)
}

pub(crate) fn render_raster_for_size(
    image: &DynamicImage,
    max_columns: u32,
    max_rows: u32,
) -> Result<String> {
    if max_columns == 0 || max_rows == 0 {
        return Ok(String::new());
    }
    let scaled = fit_dimensions(image.width(), image.height(), max_columns, max_rows);
    if scaled.0 == 0 || scaled.1 == 0 {
        return Ok(String::new());
    }

    let rgba = image.to_rgba8();
    let resized = nearest_scaled_rgba(&rgba, scaled.0, scaled.1);
    Ok(render_half_blocks(&resized, scaled.0, scaled.1))
}

pub(crate) fn render_plot(scene: &PlotScene, kind: PlotKind) -> Result<String> {
    render_plot_for_size(scene, kind, PLOT_WIDTH, PLOT_HEIGHT)
}

pub(crate) fn render_plot_for_size(
    scene: &PlotScene,
    kind: PlotKind,
    width: u32,
    height: u32,
) -> Result<String> {
    let bounds = scene.bounds().context("plot scene is empty")?;
    render_plot_for_bounds_plain(scene, kind, bounds, width, height)
}

pub(crate) fn render_terminal_plot_for_size(
    scene: &PlotScene,
    kind: PlotKind,
    bounds: PlotBounds,
    width: u32,
    height: u32,
) -> Result<String> {
    render_terminal_braille_plot(scene, kind, bounds.normalized(), width, height)
}

fn render_plot_for_bounds_plain(
    scene: &PlotScene,
    kind: PlotKind,
    bounds: PlotBounds,
    width: u32,
    height: u32,
) -> Result<String> {
    let width = width.max(1);
    let height = height.max(1);

    if width < 4 || height < 4 {
        return Ok(String::new());
    }
    let mut canvas = vec![vec![' '; width as usize]; height as usize];
    let left = 5usize.min(width as usize / 2).max(1);
    let top = 1usize;
    let right = 1usize;
    let bottom = 3usize.min(height as usize / 2).max(1);

    let chart_width = (width as isize - left as isize - right as isize).max(1) as usize;
    let chart_height = (height as isize - top as isize - bottom as isize).max(1) as usize;
    let chart_left = left.min((width as usize).saturating_sub(1));
    let chart_right = chart_left + chart_width;
    let chart_bottom = top + chart_height.min((height as usize).saturating_sub(top + 1));
    let plot_area = PlotArea {
        left: chart_left,
        right: chart_right,
        top,
        bottom: chart_bottom,
        width: chart_width as f64,
        height: chart_height as f64,
    };

    draw_axes(
        &mut canvas,
        chart_left,
        chart_right,
        top,
        chart_bottom,
        height,
    )?;
    draw_grid_data(scene, &mut canvas, bounds.normalized(), plot_area, kind);
    let legend_row = chart_bottom.saturating_add(1).min(height as usize - 1);
    draw_legend(&mut canvas, scene, left, legend_row);
    draw_title(&mut canvas, scene, &bounds);

    let mut output = String::new();
    for row in canvas {
        let text: String = row.into_iter().collect();
        output.push_str(&text);
        output.push('\n');
    }
    Ok(output)
}

pub(crate) fn fit_dimensions(
    source_width: u32,
    source_height: u32,
    max_columns: u32,
    max_rows: u32,
) -> (u32, u32) {
    if source_width == 0 || source_height == 0 {
        return (0, 0);
    }

    let max_pixel_rows = (max_rows * 2).max(1);
    let width_scale = max_columns as f64 / source_width as f64;
    let height_scale = max_pixel_rows as f64 / source_height as f64;
    let scale = width_scale.min(height_scale).clamp(0.01, 1.0);

    let scaled_width = ((source_width as f64 * scale).round()).max(1.0) as u32;
    let scaled_height = ((source_height as f64 * scale).round()).max(1.0) as u32;
    (scaled_width, scaled_height)
}

fn nearest_scaled_rgba(
    source: &image::ImageBuffer<Rgba<u8>, Vec<u8>>,
    width: u32,
    height: u32,
) -> Vec<Rgba<u8>> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let source_width = source.width() as f64;
    let source_height = source.height() as f64;
    let mut pixels = Vec::with_capacity((width * height) as usize);

    for y in 0..height {
        let source_y = ((y as f64 / height as f64) * source_height) as u32;
        let source_y = source_y.min(source.height().saturating_sub(1));
        for x in 0..width {
            let source_x = ((x as f64 / width as f64) * source_width) as u32;
            let source_x = source_x.min(source.width().saturating_sub(1));
            pixels.push(*source.get_pixel(source_x, source_y));
        }
    }
    pixels
}

fn render_half_blocks(pixels: &[Rgba<u8>], width: u32, height: u32) -> String {
    let mut lines = String::new();
    let width_usize = width as usize;
    let mut row = 0usize;
    while row < height as usize {
        for col in 0..width_usize {
            let upper = pixel_at(pixels, width_usize, col, row);
            let lower = if row + 1 < height as usize {
                pixel_at(pixels, width_usize, col, row + 1)
            } else {
                upper
            };
            append_half_block(&mut lines, upper, lower);
        }
        lines.push_str("\x1b[0m\n");
        row += 2;
    }
    lines
}

fn pixel_at(pixels: &[Rgba<u8>], width: usize, x: usize, y: usize) -> Rgba<u8> {
    let index = y.saturating_mul(width).saturating_add(x);
    pixels.get(index).copied().unwrap_or(Rgba([0, 0, 0, 255]))
}

fn append_half_block(out: &mut String, upper: Rgba<u8>, lower: Rgba<u8>) {
    let [ur, ug, ub, ua] = upper.0;
    let [lr, lg, lb, la] = lower.0;
    let upper_visible = ua >= 10;
    let lower_visible = la >= 10;

    if !upper_visible && !lower_visible {
        out.push(' ');
        return;
    }

    if upper_visible && !lower_visible {
        out.push_str(&format!("\x1b[38;2;{};{};{}m▀", ur, ug, ub));
        return;
    }

    if !upper_visible && lower_visible {
        out.push_str(&format!("\x1b[38;2;{};{};{}m▄", lr, lg, lb));
        return;
    }

    out.push_str(&format!(
        "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m▀",
        ur, ug, ub, lr, lg, lb
    ));
}

fn draw_axes(
    canvas: &mut [Vec<char>],
    chart_left: usize,
    chart_right: usize,
    top: usize,
    bottom: usize,
    height: u32,
) -> Result<()> {
    if chart_left >= chart_right || top >= bottom || bottom >= height as usize {
        return Ok(());
    }
    for cell in canvas[bottom]
        .iter_mut()
        .take(chart_right + 1)
        .skip(chart_left)
    {
        *cell = '-';
    }
    for row in canvas.iter_mut().take(bottom + 1).skip(top) {
        row[chart_left] = '|';
    }
    canvas[bottom][chart_left] = '+';
    let y_label_left = 0usize;
    canvas[top][y_label_left] = '↑';
    Ok(())
}

fn draw_grid_data(
    scene: &PlotScene,
    canvas: &mut [Vec<char>],
    bounds: PlotBounds,
    area: PlotArea,
    kind: PlotKind,
) {
    let mut series_glyphs = SERIE_GLYPHS.iter().copied().cycle();
    for series in &scene.series {
        let point_glyph = series_glyphs.next().unwrap_or('●');
        let mut previous: Option<(usize, usize)> = None;
        for point in &series.points {
            let (x, y) = map_point(point, bounds, area);
            match (kind, previous) {
                (PlotKind::Line, Some((px, py))) => {
                    draw_line(canvas, px, py, x, y, point_glyph);
                }
                _ => {
                    if !is_axis_row_col(
                        canvas,
                        x,
                        y,
                        canvas.first().map(|row| row.len()).unwrap_or(0),
                        canvas.len(),
                    ) {
                        canvas[y][x] = point_glyph;
                    }
                }
            }
            previous = Some((x, y));
        }
    }
}

fn map_point(point: &PlotPoint, bounds: PlotBounds, area: PlotArea) -> (usize, usize) {
    let x_span = (bounds.x_max - bounds.x_min).max(f64::EPSILON);
    let y_span = (bounds.y_max - bounds.y_min).max(f64::EPSILON);

    let mut x = ((point.x - bounds.x_min) / x_span * area.width).floor() as isize;
    let mut y = ((bounds.y_max - point.y) / y_span * area.height).floor() as isize;

    x = x.clamp(0, (area.right.saturating_sub(area.left)) as isize);
    y = y.clamp(0, (area.bottom.saturating_sub(area.top)) as isize);

    (area.left + x as usize, area.top + y as usize)
}

fn draw_line(
    canvas: &mut [Vec<char>],
    start_x: usize,
    start_y: usize,
    end_x: usize,
    end_y: usize,
    glyph: char,
) {
    let height = canvas.len();
    let width = canvas.first().map(|row| row.len()).unwrap_or(0);
    let dx = end_x as isize - start_x as isize;
    let dy = end_y as isize - start_y as isize;
    let steps = dx.abs().max(dy.abs()) as usize;

    if steps == 0 {
        if !is_axis_row_col(canvas, start_x, start_y, width, height)
            && start_x < width
            && start_y < height
        {
            canvas[start_y][start_x] = glyph;
        }
        return;
    }

    for step in 0..=steps {
        let t = step as f64 / steps as f64;
        let x = (start_x as f64 + (dx as f64) * t).round() as isize;
        let y = (start_y as f64 + (dy as f64) * t).round() as isize;
        if x >= 0 && y >= 0 && x < width as isize && y < height as isize {
            let ux = x as usize;
            let uy = y as usize;
            if !is_axis_row_col(canvas, ux, uy, width, height) {
                canvas[uy][ux] = glyph;
            }
        }
    }
}

fn draw_title(canvas: &mut [Vec<char>], scene: &PlotScene, bounds: &PlotBounds) {
    let mut title = String::from("plot");
    if let Some(label) = scene.title.as_deref() {
        title = label.to_owned();
    }
    let subtitle = format!(
        "{}  x:[{:.3}, {:.3}] y:[{:.3}, {:.3}] n={}",
        title,
        bounds.x_min,
        bounds.x_max,
        bounds.y_min,
        bounds.y_max,
        scene.total_points()
    );
    let row = 0usize;
    for (i, ch) in subtitle.chars().take(canvas[row].len()).enumerate() {
        canvas[row][i] = ch;
    }
}

fn draw_legend(canvas: &mut [Vec<char>], scene: &PlotScene, x: usize, y: usize) {
    if y >= canvas.len() {
        return;
    }
    let mut text = String::new();
    for (index, series) in scene.series.iter().enumerate() {
        if index > 0 {
            text.push(' ');
        }
        let glyph = SERIE_GLYPHS[index % SERIE_GLYPHS.len()];
        text.push(glyph);
        text.push(' ');
        text.push_str(&series.name);
    }
    if text.is_empty() {
        text.push_str("series: none");
    }
    for (i, ch) in text
        .chars()
        .take(canvas[y].len().saturating_sub(x))
        .enumerate()
    {
        canvas[y][x + i] = ch;
    }
}

fn is_axis_row_col(canvas: &[Vec<char>], x: usize, y: usize, width: usize, height: usize) -> bool {
    if y >= height || x >= width {
        return false;
    }
    matches!(canvas[y][x], '|' | '-' | '+')
}

#[derive(Debug, Clone, Copy)]
struct TerminalCell {
    ch: char,
    fg: (u8, u8, u8),
}

impl TerminalCell {
    fn blank() -> Self {
        Self {
            ch: ' ',
            fg: TERMINAL_TEXT,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct BrailleCell {
    mask: u8,
    series: Option<usize>,
}

fn render_terminal_braille_plot(
    scene: &PlotScene,
    kind: PlotKind,
    bounds: PlotBounds,
    width: u32,
    height: u32,
) -> Result<String> {
    let width = width.max(1) as usize;
    let height = height.max(1) as usize;
    if width < 12 || height < 7 {
        return Ok(String::new());
    }

    let mut cells = vec![vec![TerminalCell::blank(); width]; height];
    let title = format!(
        "{}  x:[{:.3}, {:.3}] y:[{:.3}, {:.3}] n={}",
        scene.title.as_deref().unwrap_or("plot"),
        bounds.x_min,
        bounds.x_max,
        bounds.y_min,
        bounds.y_max,
        scene.total_points()
    );
    write_text(&mut cells, 0, 0, &title, TERMINAL_TEXT);

    let y_labels = axis_labels(bounds.y_min, bounds.y_max);
    let y_label_width = y_labels
        .iter()
        .map(|label| label.chars().count())
        .max()
        .unwrap_or(0);
    let chart_left = y_label_width
        .saturating_add(2)
        .min(width.saturating_sub(3))
        .max(5);
    let chart_top = 1usize;
    let chart_bottom = height.saturating_sub(4).max(chart_top + 1);
    let chart_right = width.saturating_sub(1).max(chart_left + 1);
    for row in cells.iter_mut().take(chart_bottom + 1).skip(chart_top) {
        row[chart_left] = TerminalCell {
            ch: '|',
            fg: TERMINAL_AXIS,
        };
    }
    for cell in cells[chart_bottom]
        .iter_mut()
        .take(chart_right + 1)
        .skip(chart_left)
    {
        *cell = TerminalCell {
            ch: '-',
            fg: TERMINAL_AXIS,
        };
    }
    cells[chart_top][0] = TerminalCell {
        ch: '^',
        fg: TERMINAL_AXIS,
    };
    cells[chart_bottom][chart_left] = TerminalCell {
        ch: '+',
        fg: TERMINAL_AXIS,
    };
    draw_terminal_ticks(
        &mut cells,
        bounds,
        chart_left,
        chart_right,
        chart_top,
        chart_bottom,
    );

    let data_left = chart_left + 1;
    let data_right = chart_right;
    let data_top = chart_top;
    let data_bottom = chart_bottom.saturating_sub(1);
    if data_right >= data_left && data_bottom >= data_top {
        let data_width = data_right - data_left + 1;
        let data_height = data_bottom - data_top + 1;
        let mut braille = vec![vec![BrailleCell::default(); data_width]; data_height];
        draw_braille_data(scene, kind, bounds, &mut braille);
        for (row_index, row) in braille.iter().enumerate() {
            for (col_index, cell) in row.iter().enumerate() {
                if cell.mask == 0 {
                    continue;
                }
                let series_index = cell.series.unwrap_or(0);
                cells[data_top + row_index][data_left + col_index] = TerminalCell {
                    ch: braille_char(cell.mask),
                    fg: TERMINAL_SERIES[series_index % TERMINAL_SERIES.len()],
                };
            }
        }
    }

    let legend_row = chart_bottom.saturating_add(2).min(height - 1);
    write_terminal_legend(&mut cells, scene, chart_left, legend_row);

    Ok(render_terminal_cells(&cells))
}

fn draw_braille_data(
    scene: &PlotScene,
    kind: PlotKind,
    bounds: PlotBounds,
    canvas: &mut [Vec<BrailleCell>],
) {
    for (series_index, series) in scene.series.iter().enumerate() {
        match kind {
            PlotKind::Line => {
                for pair in series.points.windows(2) {
                    let Some((start, end)) = clip_segment_to_bounds(&pair[0], &pair[1], bounds)
                    else {
                        continue;
                    };
                    let (x0, y0) = map_braille_point(&start, bounds, canvas);
                    let (x1, y1) = map_braille_point(&end, bounds, canvas);
                    draw_braille_line(canvas, x0, y0, x1, y1, series_index);
                }

                if series.points.len() == 1 && is_point_in_bounds(&series.points[0], bounds) {
                    let (x, y) = map_braille_point(&series.points[0], bounds, canvas);
                    set_braille_pixel(canvas, x, y, series_index);
                }
                for point in series
                    .points
                    .iter()
                    .filter(|point| is_point_in_bounds(point, bounds))
                {
                    let (x, y) = map_braille_point(point, bounds, canvas);
                    draw_braille_disc(canvas, x, y, BRAILLE_JOIN_RADIUS, series_index);
                }
            }
            PlotKind::Scatter => {
                for point in series
                    .points
                    .iter()
                    .filter(|point| is_point_in_bounds(point, bounds))
                {
                    let (x, y) = map_braille_point(point, bounds, canvas);
                    set_braille_pixel(canvas, x, y, series_index);
                }
            }
        }
    }
}

fn map_braille_point(
    point: &PlotPoint,
    bounds: PlotBounds,
    canvas: &[Vec<BrailleCell>],
) -> (usize, usize) {
    let cell_height = canvas.len().max(1);
    let cell_width = canvas.first().map(|row| row.len()).unwrap_or(1).max(1);
    let pixel_width = (cell_width * 2).max(1);
    let pixel_height = (cell_height * 4).max(1);
    let x_span = (bounds.x_max - bounds.x_min).abs().max(f64::EPSILON);
    let y_span = (bounds.y_max - bounds.y_min).abs().max(f64::EPSILON);
    let x_ratio = ((point.x - bounds.x_min) / x_span).clamp(0.0, 1.0);
    let y_ratio = ((point.y - bounds.y_min) / y_span).clamp(0.0, 1.0);
    let x = (x_ratio * (pixel_width.saturating_sub(1)) as f64).round() as usize;
    let y = ((1.0 - y_ratio) * (pixel_height.saturating_sub(1)) as f64).round() as usize;
    (x, y)
}

fn draw_braille_line(
    canvas: &mut [Vec<BrailleCell>],
    start_x: usize,
    start_y: usize,
    end_x: usize,
    end_y: usize,
    series_index: usize,
) {
    let (width, height) = braille_pixel_dimensions(canvas);
    if width == 0 || height == 0 {
        return;
    }

    let min_x = start_x.min(end_x).saturating_sub(2);
    let max_x = start_x.max(end_x).saturating_add(2).min(width - 1);
    let min_y = start_y.min(end_y).saturating_sub(2);
    let max_y = start_y.max(end_y).saturating_add(2).min(height - 1);

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let distance = distance_to_segment(
                x as f64,
                y as f64,
                start_x as f64,
                start_y as f64,
                end_x as f64,
                end_y as f64,
            );
            if distance <= BRAILLE_LINE_RADIUS {
                set_braille_pixel(canvas, x, y, series_index);
            }
        }
    }
}

fn draw_braille_disc(
    canvas: &mut [Vec<BrailleCell>],
    center_x: usize,
    center_y: usize,
    radius: f64,
    series_index: usize,
) {
    let (width, height) = braille_pixel_dimensions(canvas);
    if width == 0 || height == 0 {
        return;
    }

    let radius_cells = radius.ceil() as usize;
    let min_x = center_x.saturating_sub(radius_cells);
    let max_x = center_x.saturating_add(radius_cells).min(width - 1);
    let min_y = center_y.saturating_sub(radius_cells);
    let max_y = center_y.saturating_add(radius_cells).min(height - 1);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f64 - center_x as f64;
            let dy = y as f64 - center_y as f64;
            if (dx * dx + dy * dy).sqrt() <= radius {
                set_braille_pixel(canvas, x, y, series_index);
            }
        }
    }
}

fn distance_to_segment(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let length_sq = dx * dx + dy * dy;
    if length_sq <= f64::EPSILON {
        return ((px - x0).powi(2) + (py - y0).powi(2)).sqrt();
    }

    let t = (((px - x0) * dx + (py - y0) * dy) / length_sq).clamp(0.0, 1.0);
    let closest_x = x0 + t * dx;
    let closest_y = y0 + t * dy;
    ((px - closest_x).powi(2) + (py - closest_y).powi(2)).sqrt()
}

fn braille_pixel_dimensions(canvas: &[Vec<BrailleCell>]) -> (usize, usize) {
    let width = canvas.first().map(|row| row.len()).unwrap_or(0) * 2;
    let height = canvas.len() * 4;
    (width, height)
}

fn set_braille_pixel(
    canvas: &mut [Vec<BrailleCell>],
    pixel_x: usize,
    pixel_y: usize,
    series_index: usize,
) {
    let cell_y = pixel_y / 4;
    let cell_x = pixel_x / 2;
    let Some(row) = canvas.get_mut(cell_y) else {
        return;
    };
    let Some(cell) = row.get_mut(cell_x) else {
        return;
    };
    cell.mask |= braille_mask(pixel_x % 2, pixel_y % 4);
    cell.series = Some(series_index);
}

fn braille_mask(x: usize, y: usize) -> u8 {
    match (x, y) {
        (0, 0) => 0x01,
        (0, 1) => 0x02,
        (0, 2) => 0x04,
        (0, 3) => 0x40,
        (1, 0) => 0x08,
        (1, 1) => 0x10,
        (1, 2) => 0x20,
        (1, 3) => 0x80,
        _ => 0,
    }
}

fn braille_char(mask: u8) -> char {
    char::from_u32(0x2800 + u32::from(mask)).unwrap_or(' ')
}

fn clip_segment_to_bounds(
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
        *t0 <= *t1
    };

    if !update(-dx, start.x - bounds.x_min, &mut t0, &mut t1)
        || !update(dx, bounds.x_max - start.x, &mut t0, &mut t1)
        || !update(-dy, start.y - bounds.y_min, &mut t0, &mut t1)
        || !update(dy, bounds.y_max - start.y, &mut t0, &mut t1)
    {
        return None;
    }

    Some((
        PlotPoint {
            x: start.x + dx * t0,
            y: start.y + dy * t0,
        },
        PlotPoint {
            x: start.x + dx * t1,
            y: start.y + dy * t1,
        },
    ))
}

fn is_point_in_bounds(point: &PlotPoint, bounds: PlotBounds) -> bool {
    (bounds.x_min..=bounds.x_max).contains(&point.x)
        && (bounds.y_min..=bounds.y_max).contains(&point.y)
}

fn write_terminal_legend(cells: &mut [Vec<TerminalCell>], scene: &PlotScene, x: usize, y: usize) {
    let mut cursor = x;
    for (index, series) in scene.series.iter().enumerate() {
        if index > 0 {
            write_text(cells, cursor, y, "  ", TERMINAL_TEXT);
            cursor = cursor.saturating_add(2);
        }
        let glyph = TERMINAL_SERIE_GLYPHS[index % TERMINAL_SERIE_GLYPHS.len()];
        write_text(
            cells,
            cursor,
            y,
            &glyph.to_string(),
            TERMINAL_SERIES[index % TERMINAL_SERIES.len()],
        );
        cursor = cursor.saturating_add(2);
        write_text(cells, cursor, y, &series.name, TERMINAL_TEXT);
        cursor = cursor.saturating_add(series.name.chars().count());
    }
    if scene.series.is_empty() {
        write_text(cells, x, y, "series: none", TERMINAL_TEXT);
    }
}

fn draw_terminal_ticks(
    cells: &mut [Vec<TerminalCell>],
    bounds: PlotBounds,
    chart_left: usize,
    chart_right: usize,
    chart_top: usize,
    chart_bottom: usize,
) {
    let y_labels = axis_labels(bounds.y_min, bounds.y_max);
    let x_span = (bounds.x_max - bounds.x_min).abs().max(f64::EPSILON);
    let steps = 5usize;

    for (step, y_label) in y_labels.iter().enumerate() {
        let ratio = step as f64 / (steps - 1) as f64;
        let row =
            chart_top + ((chart_bottom.saturating_sub(chart_top)) as f64 * ratio).round() as usize;
        let label_width = y_label.chars().count();
        let label_x = chart_left.saturating_sub(label_width + 1);
        write_text(cells, label_x, row, y_label, TERMINAL_TEXT);

        let value = bounds.x_min + x_span * ratio;
        let x_label = format_axis_label(value);
        let x =
            chart_left + ((chart_right.saturating_sub(chart_left)) as f64 * ratio).round() as usize;
        let x_label_width = x_label.chars().count();
        let x_label_x = x
            .saturating_sub(x_label_width / 2)
            .min(chart_right.saturating_sub(x_label_width.saturating_sub(1)));
        write_text(
            cells,
            x_label_x,
            chart_bottom.saturating_add(1),
            &x_label,
            TERMINAL_TEXT,
        );
    }
}

fn axis_labels(min: f64, max: f64) -> Vec<String> {
    let span = (max - min).abs().max(f64::EPSILON);
    (0..5)
        .map(|step| {
            let ratio = step as f64 / 4.0;
            format_axis_label(max - span * ratio)
        })
        .collect()
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

fn write_text(cells: &mut [Vec<TerminalCell>], x: usize, y: usize, text: &str, fg: (u8, u8, u8)) {
    let Some(row) = cells.get_mut(y) else {
        return;
    };
    for (offset, ch) in text.chars().enumerate() {
        let column = x + offset;
        let Some(cell) = row.get_mut(column) else {
            break;
        };
        *cell = TerminalCell { ch, fg };
    }
}

fn render_terminal_cells(cells: &[Vec<TerminalCell>]) -> String {
    let mut output = String::new();
    for row in cells {
        push_bg(&mut output, TERMINAL_BG);
        let mut current_fg: Option<(u8, u8, u8)> = None;
        for cell in row {
            if current_fg != Some(cell.fg) {
                push_fg(&mut output, cell.fg);
                current_fg = Some(cell.fg);
            }
            output.push(cell.ch);
        }
        output.push_str("\x1b[0m\n");
    }
    output
}

fn push_bg(output: &mut String, (r, g, b): (u8, u8, u8)) {
    output.push_str(&format!("\x1b[48;2;{r};{g};{b}m"));
}

fn push_fg(output: &mut String, (r, g, b): (u8, u8, u8)) {
    output.push_str(&format!("\x1b[38;2;{r};{g};{b}m"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn half_block_keeps_lower_visible_pixel_when_upper_is_transparent() {
        let mut image = RgbaImage::from_pixel(1, 2, Rgba([0, 0, 0, 0]));
        image.put_pixel(0, 1, Rgba([255, 0, 0, 255]));

        let output =
            render_raster_for_size(&DynamicImage::ImageRgba8(image), 1, 1).expect("render raster");

        assert!(output.contains('▄'));
        assert!(output.contains("38;2;255;0;0"));
    }

    #[test]
    fn half_block_leaves_fully_transparent_pixels_blank() {
        let image = RgbaImage::from_pixel(1, 2, Rgba([0, 0, 0, 0]));

        let output =
            render_raster_for_size(&DynamicImage::ImageRgba8(image), 1, 1).expect("render raster");

        assert_eq!(output, " \x1b[0m\n");
    }
}
