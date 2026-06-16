mod ascii;
mod braille;

use crate::plot::{PlotKind, model::PlotScene};
use anyhow::{Context, Result};

const PLOT_WIDTH: u32 = 80;
const PLOT_HEIGHT: u32 = 30;

pub(crate) fn render_plot(scene: &PlotScene, kind: PlotKind) -> Result<String> {
    render_plot_for_size(scene, kind, PLOT_WIDTH, PLOT_HEIGHT)
}

fn render_plot_for_size(
    scene: &PlotScene,
    kind: PlotKind,
    width: u32,
    height: u32,
) -> Result<String> {
    let bounds = scene.bounds().context("plot scene is empty")?;
    ascii::render(scene, kind, bounds, width, height)
}

pub(crate) fn render_terminal_plot_for_size(
    scene: &PlotScene,
    kind: PlotKind,
    bounds: crate::plot::model::PlotBounds,
    width: u32,
    height: u32,
) -> Result<String> {
    braille::render(scene, kind, bounds.normalized(), width, height)
}
