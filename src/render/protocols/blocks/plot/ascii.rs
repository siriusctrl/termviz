use crate::plot::{
    PlotKind,
    model::{PlotBounds, PlotPoint, PlotScene},
};
use anyhow::Result;

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

pub(super) fn render(
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
