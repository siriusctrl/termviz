use crate::plot::{
    PlotKind,
    model::{PlotBounds, PlotPoint, PlotScene},
};
use anyhow::Result;

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

pub(super) fn render(
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
