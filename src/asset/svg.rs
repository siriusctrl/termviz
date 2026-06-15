use anyhow::{Result, bail};

use crate::{asset::PixelDimensions, input::InputSource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SvgMetadata {
    pub(crate) viewport: Option<PixelDimensions>,
}

pub(crate) fn read_metadata(_source: &InputSource) -> Result<SvgMetadata> {
    bail!("SVG metadata loading is not implemented yet")
}
