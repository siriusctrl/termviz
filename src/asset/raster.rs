use anyhow::{Context, Result};
use image::{AnimationDecoder, ImageDecoder, ImageReader, codecs::gif::GifDecoder};
use std::io::BufReader;

use crate::{asset::PixelDimensions, input::InputSource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RasterMetadata {
    pub(crate) dimensions: Option<PixelDimensions>,
    pub(crate) frames: Option<usize>,
    pub(crate) color: Option<String>,
}

pub(crate) fn read_metadata(source: &InputSource) -> Result<RasterMetadata> {
    let reader = ImageReader::open(source.path())
        .with_context(|| format!("failed to open {} for raster metadata", source.label()))?
        .with_guessed_format()
        .context("failed to detect raster format")?;

    let decoder = reader
        .into_decoder()
        .context("failed to open raster decoder")?;
    let (width, height) = decoder.dimensions();
    let color = Some(format!("{:?}", decoder.color_type()));

    let frames = if source
        .path()
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gif"))
    {
        gif_frame_count(source).ok()
    } else {
        None
    };

    Ok(RasterMetadata {
        dimensions: Some(PixelDimensions { width, height }),
        frames,
        color,
    })
}

fn gif_frame_count(source: &InputSource) -> Result<usize> {
    let file = source
        .open()
        .context("failed to open GIF for frame count")?;
    let reader = BufReader::new(file);
    let frames = GifDecoder::new(reader)
        .context("failed to initialize GIF decoder")?
        .into_frames();
    let mut count = 0usize;

    for frame in frames {
        frame?;
        count += 1;
    }

    Ok(count)
}
