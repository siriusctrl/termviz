use crate::{
    plot::model::{PlotBounds, PlotPoint, PlotScene},
    tui::TerminalSize,
};

use super::{chrome::plot_protocol_layout, state::PlotViewState};

const MAX_HOVER_SERIES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PlotHoverCell {
    pub(super) col: u16,
    pub(super) row: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct PlotHover {
    pub(super) x: f64,
    pub(super) samples: Vec<PlotHoverSample>,
    pub(super) hidden_samples: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct PlotHoverSample {
    pub(super) label: String,
    pub(super) point: PlotPoint,
}

pub(super) fn hover_for_cell(
    scene: &PlotScene,
    state: &PlotViewState,
    size: TerminalSize,
    cell: PlotHoverCell,
) -> Option<PlotHover> {
    let layout = plot_protocol_layout(size);
    if cell.col < layout.image_col
        || cell.col >= layout.image_col.saturating_add(layout.image_cols)
        || cell.row < layout.image_row
        || cell.row >= layout.image_row.saturating_add(layout.image_rows)
    {
        return None;
    }

    let x_ratio = if layout.image_cols <= 1 {
        0.0
    } else {
        f64::from(cell.col.saturating_sub(layout.image_col))
            / f64::from(layout.image_cols.saturating_sub(1))
    };
    let x = state.visible.x_min + (state.visible.x_max - state.visible.x_min) * x_ratio;

    hover_for_x(scene, state.visible, x)
}

pub(super) fn hover_for_x(scene: &PlotScene, visible: PlotBounds, x: f64) -> Option<PlotHover> {
    let snapped_x = nearest_visible_point(scene, visible, x)
        .map(|point| point.x)
        .unwrap_or(x);
    let mut samples = Vec::new();
    let mut hidden_samples = 0usize;
    for (index, series) in scene.series.iter().enumerate() {
        let Some(point) = series
            .points
            .iter()
            .filter(|point| point_is_visible(point, visible))
            .min_by(|left, right| {
                (left.x - snapped_x)
                    .abs()
                    .total_cmp(&(right.x - snapped_x).abs())
                    .then_with(|| left.y.total_cmp(&right.y))
            })
            .copied()
        else {
            continue;
        };

        if samples.len() < MAX_HOVER_SERIES {
            let label = if series.name.is_empty() {
                format!("series {}", index + 1)
            } else {
                series.name.clone()
            };
            samples.push(PlotHoverSample { label, point });
        } else {
            hidden_samples += 1;
        }
    }

    Some(PlotHover {
        x: snapped_x,
        samples,
        hidden_samples,
    })
}

fn nearest_visible_point(scene: &PlotScene, visible: PlotBounds, x: f64) -> Option<PlotPoint> {
    scene
        .series
        .iter()
        .flat_map(|series| series.points.iter())
        .filter(|point| point_is_visible(point, visible))
        .min_by(|left, right| {
            (left.x - x)
                .abs()
                .total_cmp(&(right.x - x).abs())
                .then_with(|| left.y.total_cmp(&right.y))
        })
        .copied()
}

fn point_is_visible(point: &PlotPoint, visible: PlotBounds) -> bool {
    point.x >= visible.x_min
        && point.x <= visible.x_max
        && point.y >= visible.y_min
        && point.y <= visible.y_max
}

pub(super) fn format_hover_value(value: f64) -> String {
    if value.abs() >= 10_000.0 {
        format!("{value:.1e}")
    } else if value.fract().abs() < 0.001 {
        format!("{value:.0}")
    } else if value.abs() >= 100.0 {
        format!("{value:.1}")
    } else if value.abs() >= 10.0 {
        format!("{value:.2}")
    } else {
        format!("{value:.3}")
    }
}
