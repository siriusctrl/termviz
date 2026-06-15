use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

use crate::asset::{raster, svg};
use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, ValueEnum};

use crate::{
    export::{ExportFormat, ExportRequest},
    input::InputSource,
    plot::PlotKind,
    profile::{ContentKind, InputProfile, RenderStrategy},
    render::Protocol,
};

const INTERACTIVE_RASTER_PIXEL_LIMIT: u64 = 8_000_000;

#[derive(Debug, Parser)]
#[command(version, about = "Terminal-first viewer for images and plots")]
pub struct Args {
    /// Image, data table, data stream, or future plot spec to inspect/view.
    pub input: PathBuf,

    /// Print the resolved profile instead of opening the interactive viewer.
    #[arg(long)]
    pub inspect: bool,

    /// Terminal image protocol to use for interactive rendering.
    #[arg(long, value_enum, default_value_t = ProtocolArg::Auto)]
    pub protocol: ProtocolArg,

    /// Output path for explicit export work.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Explicit export format. Protocol output is never implicit on redirect.
    #[arg(long, value_enum)]
    pub format: Option<ExportFormatArg>,

    /// X column or field for table/stream plots.
    #[arg(long)]
    pub x: Option<String>,

    /// Y column or field for table/stream plots.
    #[arg(long)]
    pub y: Option<String>,

    /// Optional grouping column or field for plot series.
    #[arg(long)]
    pub group: Option<String>,

    /// Plot kind for data inputs.
    #[arg(long, value_enum, default_value_t = PlotKindArg::Line)]
    pub kind: PlotKindArg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProtocolArg {
    Auto,
    Kitty,
    Sixel,
    Iterm,
    Blocks,
}

impl From<ProtocolArg> for Protocol {
    fn from(value: ProtocolArg) -> Self {
        match value {
            ProtocolArg::Auto => Self::Auto,
            ProtocolArg::Kitty => Self::Kitty,
            ProtocolArg::Sixel => Self::Sixel,
            ProtocolArg::Iterm => Self::Iterm,
            ProtocolArg::Blocks => Self::Blocks,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ExportFormatArg {
    Png,
    Svg,
    Ansi,
    Json,
}

impl From<ExportFormatArg> for ExportFormat {
    fn from(value: ExportFormatArg) -> Self {
        match value {
            ExportFormatArg::Png => Self::Png,
            ExportFormatArg::Svg => Self::Svg,
            ExportFormatArg::Ansi => Self::Ansi,
            ExportFormatArg::Json => Self::Json,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PlotKindArg {
    Line,
    Scatter,
}

impl From<PlotKindArg> for PlotKind {
    fn from(value: PlotKindArg) -> Self {
        match value {
            PlotKindArg::Line => Self::Line,
            PlotKindArg::Scatter => Self::Scatter,
        }
    }
}

pub fn run() -> Result<()> {
    run_with(Args::parse(), &mut io::stdout())
}

pub fn run_with(args: Args, stdout: &mut dyn Write) -> Result<()> {
    let source = InputSource::from_path(args.input)?;
    let profile = InputProfile::resolve(&source, args.kind.into())?;

    if args.inspect {
        writeln!(stdout, "{}", profile.inspect_text())?;
        write_inspect_metadata(stdout, &profile, &source)?;
        return Ok(());
    }

    let protocol = args.protocol.into();
    let kind = args.kind.into();
    let x = args.x;
    let y = args.y;
    let group = args.group;

    if args.output.is_some() || args.format.is_some() {
        let request = ExportRequest {
            path: args.output,
            format: args.format.map(Into::into),
            x,
            y,
            group,
            kind,
        };
        return crate::export::run(&source, &profile, request, stdout);
    }

    if !io::stdout().is_terminal() {
        bail!(
            "interactive viewing requires a TTY; use --inspect or explicit --output/--format for scriptable output"
        );
    }

    if profile.render == RenderStrategy::TerminalImage {
        enforce_interactive_viewing_constraints(&profile, &source)?;
    }

    crate::viewer::run(
        source,
        profile,
        crate::viewer::ViewerRequest {
            protocol,
            x,
            y,
            group,
            kind,
        },
    )
}

fn enforce_interactive_viewing_constraints(
    profile: &InputProfile,
    source: &InputSource,
) -> Result<()> {
    match profile.content {
        ContentKind::Png | ContentKind::Jpeg | ContentKind::Gif | ContentKind::Webp => {
            enforce_interactive_raster_limits(source)
        }
        ContentKind::Svg => bail!(
            "interactive SVG viewing/rasterization is not implemented yet; use --inspect for metadata or --output/--format svg for explicit export"
        ),
        _ => Ok(()),
    }
}

fn enforce_interactive_raster_limits(source: &InputSource) -> Result<()> {
    let metadata =
        raster::read_metadata(source).context("reading raster metadata before interactive open")?;
    let dimensions = metadata
        .dimensions
        .ok_or_else(|| anyhow!("missing raster dimensions before interactive open"))?;

    if is_large_raster(
        dimensions.width,
        dimensions.height,
        INTERACTIVE_RASTER_PIXEL_LIMIT,
    ) {
        bail!(
            "interactive image viewing is not yet tile-based; this raster is {}x{} which exceeds the {}-pixel safety guard. Use --format/--output (or --inspect) for large inputs.",
            dimensions.width,
            dimensions.height,
            INTERACTIVE_RASTER_PIXEL_LIMIT
        );
    }

    Ok(())
}

fn write_inspect_metadata(
    stdout: &mut dyn Write,
    profile: &InputProfile,
    source: &InputSource,
) -> Result<()> {
    match profile.content {
        ContentKind::Png | ContentKind::Jpeg | ContentKind::Gif | ContentKind::Webp => {
            let metadata = raster::read_metadata(source).ok();
            let dimensions = metadata
                .as_ref()
                .and_then(|metadata| metadata.dimensions)
                .map(|size| format!("{}x{}", size.width, size.height))
                .unwrap_or_else(|| "unknown".to_owned());
            let color = metadata
                .as_ref()
                .and_then(|metadata| metadata.color.as_deref())
                .unwrap_or("unknown");
            let frames = metadata
                .as_ref()
                .and_then(|metadata| metadata.frames)
                .map(|frames| frames.to_string())
                .unwrap_or_else(|| "unknown".to_owned());

            writeln!(stdout, "dimensions={dimensions}")?;
            writeln!(stdout, "color={color}")?;
            writeln!(stdout, "frames={frames}")?;
        }
        ContentKind::Svg => {
            let viewport = svg::read_metadata(source)
                .ok()
                .and_then(|metadata| metadata.viewport)
                .map(|size| format!("{}x{}", size.width, size.height))
                .unwrap_or_else(|| "unknown".to_owned());
            writeln!(stdout, "viewport={viewport}")?;
        }
        _ => {}
    }

    Ok(())
}

fn is_large_raster(width: u32, height: u32, limit: u64) -> bool {
    width.checked_mul(height).map(u64::from).unwrap_or(u64::MAX) > limit
}

#[cfg(test)]
mod tests {
    use super::{enforce_interactive_viewing_constraints, is_large_raster};
    use crate::input::InputSource;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn small_raster_is_under_interactive_limit() {
        assert!(!is_large_raster(1024, 1024, 8_000_000));
    }

    #[test]
    fn large_raster_is_flagged() {
        assert!(is_large_raster(3000, 3000, 8_000_000));
    }

    #[test]
    fn interactive_svg_returns_clear_message() {
        let mut file = NamedTempFile::with_suffix(".svg").unwrap();
        writeln!(
            file,
            r#"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\" fill=\"none\"/></svg>"#
        )
        .unwrap();
        let source = InputSource::from_path(file.path().to_path_buf()).unwrap();
        let profile =
            crate::profile::InputProfile::resolve(&source, crate::plot::PlotKind::Line).unwrap();
        assert_eq!(profile.content, crate::profile::ContentKind::Svg);
        assert_eq!(
            profile.render,
            crate::profile::RenderStrategy::TerminalImage
        );

        let err = enforce_interactive_viewing_constraints(&profile, &source).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("interactive SVG"));
        assert!(message.contains("--inspect"));
        assert!(message.contains("--format svg"));
    }
}
