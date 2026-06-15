use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

use crate::asset::{raster, svg};
use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};

use crate::{
    export::{ExportFormat, ExportRequest},
    input::InputSource,
    plot::PlotKind,
    profile::{ContentKind, InputProfile},
    render::Protocol,
};

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

    if args.output.is_some() || args.format.is_some() {
        let request = ExportRequest {
            path: args.output,
            format: args.format.map(Into::into),
            x: args.x,
            y: args.y,
            group: args.group,
            kind: args.kind.into(),
        };
        return crate::export::run(&source, &profile, request, stdout);
    }

    if !io::stdout().is_terminal() {
        bail!(
            "interactive viewing requires a TTY; use --inspect or explicit --output/--format for scriptable output"
        );
    }

    crate::viewer::run(source, profile, args.protocol.into())
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
