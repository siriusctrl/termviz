use crate::plot::model::PlotScene;

use super::text::TextMetrics;

const BASE_LEFT_MARGIN: i32 = 74;
const BASE_RIGHT_MARGIN: i32 = 18;
const BASE_TOP_MARGIN: i32 = 78;
const BASE_BOTTOM_MARGIN: i32 = 50;
const BASE_HEADER_PADDING: i32 = 12;
const BASE_TITLE_Y: i32 = 8;
const BASE_LEGEND_TOP: i32 = 24;
const BASE_LEGEND_MAX_ROWS: usize = 4;
const BASE_LEGEND_MIN_WIDTH: i32 = 126;
const BASE_LEGEND_MAX_WIDTH: i32 = 220;
const BASE_LEGEND_ROW_HEIGHT: i32 = 10;
const BASE_LEGEND_SWATCH_WIDTH: i32 = 14;
const BASE_LEGEND_TEXT_GAP: i32 = 6;
const BASE_HEADER_GAP: i32 = 8;
const BASE_Y_TICK_GAP: i32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PlotDimensions {
    pub(super) width: u32,
    pub(super) height: u32,
}

impl PlotDimensions {
    pub(super) fn new(width: u32, height: u32) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
        }
    }

    fn width_i32(&self) -> i32 {
        i32::try_from(self.width).unwrap_or(i32::MAX)
    }

    fn height_i32(&self) -> i32 {
        i32::try_from(self.height).unwrap_or(i32::MAX)
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PlotArea {
    pub(super) left: i32,
    pub(super) right: i32,
    pub(super) top: i32,
    pub(super) bottom: i32,
    pub(super) width: f64,
    pub(super) height: f64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LegendArea {
    pub(super) left: i32,
    pub(super) width: i32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PlotLayout {
    pub(super) dimensions: PlotDimensions,
    pub(super) area: PlotArea,
    pub(super) legend: LegendArea,
    pub(super) text: TextMetrics,
    pub(super) header_padding: i32,
    pub(super) title_y: i32,
    pub(super) legend_top: i32,
    pub(super) legend_max_rows: usize,
    pub(super) legend_row_height: i32,
    pub(super) legend_swatch_width: i32,
    pub(super) legend_text_gap: i32,
    pub(super) y_tick_gap: i32,
}

pub(super) fn export_dimensions() -> PlotDimensions {
    PlotDimensions::new(640, 360)
}

pub(super) fn interactive_text_scale(dimensions: PlotDimensions) -> u32 {
    if dimensions.width >= 900 && dimensions.height >= 480 {
        2
    } else {
        1
    }
}

pub(super) fn layout_for(
    dimensions: PlotDimensions,
    scene: &PlotScene,
    text: TextMetrics,
) -> PlotLayout {
    let scale = text.scale();
    let header_padding = BASE_HEADER_PADDING * scale;
    let title_y = BASE_TITLE_Y * scale;
    let legend_top = BASE_LEGEND_TOP * scale;
    let legend_row_height = BASE_LEGEND_ROW_HEIGHT * scale;
    let legend_swatch_width = BASE_LEGEND_SWATCH_WIDTH * scale;
    let legend_text_gap = BASE_LEGEND_TEXT_GAP * scale;
    let y_tick_gap = BASE_Y_TICK_GAP * scale;
    let legend_max_rows = BASE_LEGEND_MAX_ROWS;
    let legend = legend_area(dimensions, scene, text, header_padding);

    let canvas_width = dimensions.width_i32();
    let canvas_height = dimensions.height_i32();
    let legend_bottom = legend_top
        + (scene.series.len().min(legend_max_rows).saturating_sub(1) as i32) * legend_row_height
        + text.glyph_height();
    let top_preferred = (BASE_TOP_MARGIN * scale).max(legend_bottom + BASE_HEADER_GAP * scale);
    let bottom_preferred = canvas_height
        .saturating_sub(BASE_BOTTOM_MARGIN * scale)
        .saturating_sub(1);

    let left_preferred = (BASE_LEFT_MARGIN * scale)
        .max(text.width("10000.0") + y_tick_gap + 2)
        .max(1);
    let right_preferred = canvas_width
        .saturating_sub(BASE_RIGHT_MARGIN * scale)
        .saturating_sub(1);

    let left = left_preferred
        .min((canvas_width.saturating_sub(3) / 2).max(1))
        .max(1);
    let right = right_preferred.max(left + 1).min(canvas_width - 1);
    let bottom = bottom_preferred
        .max(top_preferred + 1)
        .min(canvas_height - 1);
    let top = top_preferred.min(bottom.saturating_sub(1)).max(1);

    let area = PlotArea {
        left,
        right,
        top,
        bottom,
        width: f64::from((right - left).max(1)),
        height: f64::from((bottom - top).max(1)),
    };

    PlotLayout {
        dimensions,
        area,
        legend,
        text,
        header_padding,
        title_y,
        legend_top,
        legend_max_rows,
        legend_row_height,
        legend_swatch_width,
        legend_text_gap,
        y_tick_gap,
    }
}

pub(super) fn body_layout_for(dimensions: PlotDimensions, text: TextMetrics) -> PlotLayout {
    let canvas_width = dimensions.width_i32();
    let canvas_height = dimensions.height_i32();
    let inset = (2 * text.scale()).max(2);
    let left = inset
        .min((canvas_width.saturating_sub(3) / 2).max(1))
        .max(1);
    let top = inset
        .min((canvas_height.saturating_sub(3) / 2).max(1))
        .max(1);
    let right = canvas_width
        .saturating_sub(inset)
        .saturating_sub(1)
        .max(left + 1);
    let bottom = canvas_height
        .saturating_sub(inset)
        .saturating_sub(1)
        .max(top + 1);

    PlotLayout {
        dimensions,
        area: PlotArea {
            left,
            right,
            top,
            bottom,
            width: f64::from((right - left).max(1)),
            height: f64::from((bottom - top).max(1)),
        },
        legend: LegendArea {
            left: right,
            width: 0,
        },
        text,
        header_padding: inset,
        title_y: 0,
        legend_top: 0,
        legend_max_rows: 0,
        legend_row_height: 0,
        legend_swatch_width: 0,
        legend_text_gap: 0,
        y_tick_gap: 0,
    }
}

pub(super) fn legend_label(index: usize, series: &crate::plot::model::PlotSeries) -> String {
    let name = if series.name.is_empty() {
        format!("series {}", index + 1)
    } else {
        series.name.clone()
    };
    let label = truncate_chars(&name, 22);
    format!("{label} ({} pts)", series.points.len())
}

fn legend_area(
    dimensions: PlotDimensions,
    scene: &PlotScene,
    text: TextMetrics,
    header_padding: i32,
) -> LegendArea {
    let widest_label = scene
        .series
        .iter()
        .take(BASE_LEGEND_MAX_ROWS)
        .enumerate()
        .map(|(index, series)| text.width(&legend_label(index, series)))
        .max()
        .unwrap_or(0);
    let scale = text.scale();
    let width = (BASE_LEGEND_SWATCH_WIDTH * scale + BASE_LEGEND_TEXT_GAP * scale + widest_label)
        .clamp(BASE_LEGEND_MIN_WIDTH * scale, BASE_LEGEND_MAX_WIDTH * scale);
    let canvas_width = dimensions.width_i32();

    LegendArea {
        left: canvas_width
            .saturating_sub(header_padding)
            .saturating_sub(width),
        width,
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}
