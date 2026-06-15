use anyhow::{Result, bail};

use crate::{asset::PixelDimensions, input::InputSource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RasterMetadata {
    pub(crate) dimensions: Option<PixelDimensions>,
    pub(crate) frames: Option<usize>,
}

pub(crate) fn read_metadata(_source: &InputSource) -> Result<RasterMetadata> {
    bail!("raster metadata loading is not implemented yet")
}
