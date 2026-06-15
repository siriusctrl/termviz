use std::{io::Write, path::PathBuf};

use anyhow::{Result, bail};

use crate::{input::InputSource, profile::InputProfile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExportFormat {
    Png,
    Svg,
    Ansi,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExportRequest {
    pub(crate) path: Option<PathBuf>,
    pub(crate) format: Option<ExportFormat>,
}

pub(crate) fn run(
    _source: &InputSource,
    _profile: &InputProfile,
    request: ExportRequest,
    stdout: &mut dyn Write,
) -> Result<()> {
    if request.format == Some(ExportFormat::Json) && request.path.is_none() {
        writeln!(stdout, "{{\"status\":\"export metadata not implemented\"}}")?;
        return Ok(());
    }
    bail!("export is not implemented yet")
}
