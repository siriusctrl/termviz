use std::{
    ffi::OsStr,
    io::{BufReader, Read},
};

use anyhow::{Context, Result};

use super::InputSource;

const SNIFF_BYTES: usize = 64 * 1024;
const SNIFF_LINES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExtensionHint {
    Png,
    Jpeg,
    Gif,
    Webp,
    Svg,
    Csv,
    Tsv,
    Jsonl,
    Unknown,
}

pub(crate) fn extension_hint(source: &InputSource) -> ExtensionHint {
    let Some(extension) = source
        .path()
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
    else {
        return ExtensionHint::Unknown;
    };

    match extension.as_str() {
        "png" => ExtensionHint::Png,
        "jpg" | "jpeg" => ExtensionHint::Jpeg,
        "gif" => ExtensionHint::Gif,
        "webp" => ExtensionHint::Webp,
        "svg" => ExtensionHint::Svg,
        "csv" => ExtensionHint::Csv,
        "tsv" => ExtensionHint::Tsv,
        "jsonl" | "ndjson" => ExtensionHint::Jsonl,
        _ => ExtensionHint::Unknown,
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContentSample {
    pub(crate) first_non_ws: Option<u8>,
    pub(crate) lines: usize,
    pub(crate) comma_lines: usize,
    pub(crate) tab_lines: usize,
    pub(crate) jsonl_lines: usize,
    pub(crate) looks_like_svg: bool,
}

impl ContentSample {
    pub(crate) fn read(source: &InputSource) -> Result<Self> {
        let mut reader = BufReader::new(source.open()?);
        let mut prefix = Vec::new();
        reader
            .by_ref()
            .take(SNIFF_BYTES as u64)
            .read_to_end(&mut prefix)
            .with_context(|| format!("failed to inspect {}", source.label()))?;

        let mut sample = Self {
            first_non_ws: prefix
                .iter()
                .copied()
                .find(|byte| !byte.is_ascii_whitespace()),
            looks_like_svg: prefix
                .windows(4)
                .any(|window| window.eq_ignore_ascii_case(b"<svg")),
            ..Self::default()
        };

        for line in prefix.split(|byte| *byte == b'\n').take(SNIFF_LINES) {
            let trimmed = trim_ascii(line);
            if trimmed.is_empty() {
                continue;
            }
            sample.lines += 1;
            if trimmed.contains(&b',') {
                sample.comma_lines += 1;
            }
            if trimmed.contains(&b'\t') {
                sample.tab_lines += 1;
            }
            if looks_like_json_record(trimmed) {
                sample.jsonl_lines += 1;
            }
        }

        Ok(sample)
    }
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|index| index + 1)
        .unwrap_or(start);
    &bytes[start..end]
}

fn looks_like_json_record(bytes: &[u8]) -> bool {
    matches!(bytes.first(), Some(b'{' | b'[')) && matches!(bytes.last(), Some(b'}' | b']'))
}
