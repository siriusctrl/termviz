pub(crate) mod raster;
pub(crate) mod svg;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PixelDimensions {
    pub(crate) width: u32,
    pub(crate) height: u32,
}
