use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{ColorType, DynamicImage, ImageEncoder, RgbaImage, codecs::png::PngEncoder};

use crate::render::Protocol;

pub(crate) mod blocks;
pub(crate) mod kitty;
pub(crate) mod plot;

const PNG_CHUNK_BYTES: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProtocolRenderContext {
    pub(crate) protocol: Protocol,
}

impl ProtocolRenderContext {
    pub(crate) fn new(protocol: Protocol) -> Self {
        Self { protocol }
    }
}

pub(crate) fn render_raster(
    context: ProtocolRenderContext,
    image: &DynamicImage,
    max_columns: u32,
    max_rows: u32,
) -> Result<String> {
    match context.protocol {
        Protocol::Blocks => blocks::render_raster_for_size(image, max_columns, max_rows),
        Protocol::Kitty => kitty::render_for_size(image, max_columns, max_rows),
        Protocol::Auto => unreachable!("auto protocol should be resolved before rendering"),
    }
}

pub(crate) fn render_raster_with_fallback(
    context: ProtocolRenderContext,
    image: &DynamicImage,
    max_columns: u32,
    max_rows: u32,
) -> String {
    match render_raster(context, image, max_columns, max_rows) {
        Ok(payload) => payload,
        Err(error) => {
            let fallback =
                blocks::render_raster_for_size(image, max_columns, max_rows).unwrap_or_default();
            format!("{}\nprotocol-fallback: {}", fallback, error)
        }
    }
}

pub(crate) fn render_raster_rgba_with_fallback(
    context: ProtocolRenderContext,
    image: &RgbaImage,
    max_columns: u32,
    max_rows: u32,
) -> String {
    let payload = match context.protocol {
        Protocol::Kitty => kitty::render_rgba_for_size(image, max_columns, max_rows),
        Protocol::Blocks => blocks::render_raster_for_size(
            &DynamicImage::ImageRgba8(image.clone()),
            max_columns,
            max_rows,
        ),
        Protocol::Auto => unreachable!("auto protocol should be resolved before rendering"),
    };
    match payload {
        Ok(payload) => payload,
        Err(error) => {
            let fallback = blocks::render_raster_for_size(
                &DynamicImage::ImageRgba8(image.clone()),
                max_columns,
                max_rows,
            )
            .unwrap_or_default();
            format!("{}\nprotocol-fallback: {}", fallback, error)
        }
    }
}

pub(crate) fn render_plot_rgba_with_fallback(
    context: ProtocolRenderContext,
    image: &RgbaImage,
    max_columns: u32,
    max_rows: u32,
) -> String {
    let payload = match context.protocol {
        Protocol::Kitty => kitty::render_rgba_zlib_for_size(image, max_columns, max_rows),
        Protocol::Blocks => blocks::render_raster_for_size(
            &DynamicImage::ImageRgba8(image.clone()),
            max_columns,
            max_rows,
        ),
        Protocol::Auto => unreachable!("auto protocol should be resolved before rendering"),
    };
    match payload {
        Ok(payload) => payload,
        Err(error) => {
            let fallback = blocks::render_raster_for_size(
                &DynamicImage::ImageRgba8(image.clone()),
                max_columns,
                max_rows,
            )
            .unwrap_or_default();
            format!("{}\nprotocol-fallback: {}", fallback, error)
        }
    }
}

pub(crate) fn protocol_name(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Auto => "protocol: auto",
        Protocol::Kitty => "protocol: kitty",
        Protocol::Blocks => "protocol: blocks",
    }
}

pub(crate) fn encode_png(image: &DynamicImage) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let rgba = image.to_rgba8();
    encode_rgba_png_into(&rgba, &mut output)?;
    Ok(output)
}

fn encode_rgba_png_into(rgba: &RgbaImage, output: &mut Vec<u8>) -> Result<()> {
    let encoder = PngEncoder::new(output);
    encoder
        .write_image(rgba, rgba.width(), rgba.height(), ColorType::Rgba8.into())
        .map_err(|error| anyhow!("{error}"))?;
    Ok(())
}

pub(crate) fn encode_png_rgba(image: &RgbaImage) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    encode_rgba_png_into(image, &mut output)?;
    Ok(output)
}

pub(crate) fn encode_png_base64(image: &DynamicImage) -> Result<String> {
    let bytes = encode_png(image)?;
    Ok(STANDARD.encode(bytes))
}

pub(crate) fn chunked_base64_payload(base64_payload: &str, chunk_size: usize) -> Vec<&str> {
    if base64_payload.is_empty() {
        return Vec::new();
    }

    base64_payload
        .as_bytes()
        .chunks(chunk_size)
        .map(|chunk| {
            // SAFETY: base64 payload is ASCII, so splitting UTF-8 bytes at arbitrary points
            // preserves UTF-8 boundaries.
            std::str::from_utf8(chunk).unwrap()
        })
        .collect()
}

pub(crate) fn png_chunked_base64(image: &DynamicImage) -> Result<Vec<String>> {
    let encoded = encode_png_base64(image)?;
    chunk_png_base64(encoded)
}

pub(crate) fn png_chunked_base64_rgba(image: &RgbaImage) -> Result<Vec<String>> {
    let encoded = STANDARD.encode(encode_png_rgba(image)?);
    chunk_png_base64(encoded)
}

fn chunk_png_base64(encoded: String) -> Result<Vec<String>> {
    let chunks = if encoded.len() > PNG_CHUNK_BYTES {
        encoded
            .as_bytes()
            .chunks(PNG_CHUNK_BYTES)
            .map(|chunk| {
                // SAFETY: base64 payload is ASCII, so chunking by bytes keeps UTF-8 valid.
                String::from_utf8(chunk.to_vec()).expect("base64 chunk is valid utf8")
            })
            .collect()
    } else {
        vec![encoded]
    };

    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgba};

    use super::*;

    #[test]
    fn render_raster_dispatches_every_explicit_protocol_backend() {
        let image =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(4, 4, Rgba([24, 120, 220, 255])));

        let cases = [
            (Protocol::Blocks, "\x1b[38;2;"),
            (Protocol::Kitty, "\x1b_G"),
        ];

        for (protocol, marker) in cases {
            let payload = render_raster(ProtocolRenderContext::new(protocol), &image, 12, 8)
                .unwrap_or_else(|error| panic!("{protocol:?} backend failed: {error}"));

            assert!(
                payload.contains(marker),
                "{protocol:?} payload did not contain expected marker {marker:?}: {payload:?}"
            );
        }
    }
}
