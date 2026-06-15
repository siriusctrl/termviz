use anyhow::{Result, bail};

use crate::{input::InputSource, plot::model::PlotScene};

pub(crate) fn load_scene(_source: &InputSource) -> Result<PlotScene> {
    bail!("stream plot loading is not implemented yet")
}
