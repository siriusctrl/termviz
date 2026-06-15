use std::io::Read;

use anyhow::{Context, Result};

use crate::{asset::PixelDimensions, input::InputSource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SvgMetadata {
    pub(crate) viewport: Option<PixelDimensions>,
}

const SVG_PREFIX_BYTES: usize = 64 * 1024;

pub(crate) fn read_metadata(source: &InputSource) -> Result<SvgMetadata> {
    let mut file = source
        .open()
        .with_context(|| format!("failed to open {} for SVG metadata", source.label()))?;
    let mut bytes = vec![0u8; SVG_PREFIX_BYTES];
    let bytes_len = file.read(&mut bytes)?;
    let prefix = String::from_utf8_lossy(&bytes[..bytes_len]);
    let tag = first_svg_tag(&prefix).context("failed to locate an inline <svg> tag")?;

    let width = parse_dimension_attribute(&tag, "width");
    let height = parse_dimension_attribute(&tag, "height");
    let viewport = match (width, height) {
        (Some(width), Some(height)) => Some(PixelDimensions { width, height }),
        _ => parse_view_box(&tag),
    };

    Ok(SvgMetadata { viewport })
}

fn first_svg_tag(prefix: &str) -> Result<String> {
    let lower = prefix.to_ascii_lowercase();
    let start = lower
        .find("<svg")
        .context("input does not contain an <svg> tag in its metadata prefix")?;
    let end = lower[start..]
        .find('>')
        .context("SVG <svg> start tag is incomplete in metadata prefix")?;

    Ok(prefix[start..start + end + 1].to_owned())
}

fn parse_dimension_attribute(svg_tag: &str, name: &str) -> Option<u32> {
    let value = find_attribute_value(svg_tag, name)?;
    parse_svg_dimension(value)
}

fn parse_view_box(svg_tag: &str) -> Option<PixelDimensions> {
    let value = find_attribute_value(svg_tag, "viewbox")?;
    let values: Vec<_> = value
        .split([' ', ','])
        .filter(|value| !value.is_empty())
        .collect();

    if values.len() < 4 {
        return None;
    }

    let width = parse_svg_dimension(values[2])?;
    let height = parse_svg_dimension(values[3])?;
    Some(PixelDimensions { width, height })
}

fn find_attribute_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    let lower_name = name.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut offset = 0usize;

    while offset < lower.len() {
        let found = lower[offset..].find(lower_name.as_str())?;
        let start = offset + found;
        if let Some(prev) = start.checked_sub(1).map(|index| bytes[index]) {
            if !(prev.is_ascii_whitespace() || prev == b'<' || prev == b'/' || prev == b'\n') {
                offset = start + lower_name.len();
                continue;
            }
        }

        let after = start + lower_name.len();
        if after >= bytes.len() || (!bytes[after].is_ascii_whitespace() && bytes[after] != b'=') {
            offset = after + 1;
            continue;
        }

        let mut cursor = after;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'=' {
            offset = cursor + 1;
            continue;
        }
        cursor += 1;

        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] == b'/' {
            return None;
        }
        if bytes[cursor] != b'"' && bytes[cursor] != b'\'' {
            offset = cursor + 1;
            continue;
        }

        let quote = bytes[cursor];
        let value_start = cursor + 1;
        let remainder = &lower[value_start..];
        let value_end = value_start + remainder.find(quote as char)?;

        return Some(&tag[value_start..value_end]);
    }

    None
}

fn parse_svg_dimension(value: &str) -> Option<u32> {
    let value = value.trim();
    if value.is_empty() || value.ends_with('%') {
        return None;
    }

    let value = value.trim_end_matches(['p', 'x', 'P', 'X']).trim();
    let pixels: f32 = value.parse().ok()?;
    if !(pixels.is_finite()) || pixels <= 0.0 {
        return None;
    }
    if pixels > u32::MAX as f32 {
        return None;
    }

    Some(pixels.round() as u32)
}
