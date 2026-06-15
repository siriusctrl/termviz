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
    draw_grid_data(scene, &mut canvas, bounds, plot_area, kind);
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
    let [lr, lg, lb, _la] = lower.0;
    if ua == 0 && ur == 0 && ug == 0 && ub == 0 && lr == 0 && lg == 0 && lb == 0 {
        out.push(' ');
        return;
    }
    if ua < 10 {
        out.push(' ');
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
