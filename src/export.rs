use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use image::ImageReader;
use serde_json::{Value, json};

use crate::{
    asset::{raster, svg},
    input::InputSource,
    plot::{PlotKind, model::PlotBounds, model::PlotScene, stream, table},
    profile::{ContentKind, InputProfile},
    render::{protocols, protocols::blocks},
};

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
    pub(crate) output_format: Option<ExportFormat>,
    pub(crate) x: Option<String>,
    pub(crate) y: Option<String>,
    pub(crate) group: Option<String>,
    pub(crate) kind: PlotKind,
}

pub(crate) fn run(
    source: &InputSource,
    profile: &InputProfile,
    request: ExportRequest,
    stdout: &mut dyn Write,
) -> Result<()> {
    let output_format = resolve_export_format(request.output_format, request.path.as_deref());

    let payload = match output_format {
        ExportFormat::Json => build_json_payload(
            source,
            profile,
            request.x.as_deref(),
            request.y.as_deref(),
            request.group.as_deref(),
            request.kind,
        )?,
        ExportFormat::Ansi => build_ansi_payload(
            source,
            profile,
            request.x.as_deref(),
            request.y.as_deref(),
            request.group.as_deref(),
            request.kind,
        )?,
        ExportFormat::Png => build_png_payload(
            source,
            profile,
            request.x.as_deref(),
            request.y.as_deref(),
            request.group.as_deref(),
            request.kind,
        )?,
        ExportFormat::Svg => build_svg_payload(
            source,
            profile,
            request.x.as_deref(),
            request.y.as_deref(),
            request.group.as_deref(),
            request.kind,
        )?,
    };

    write_payload(request.path, &payload, stdout)
}

fn resolve_export_format(explicit: Option<ExportFormat>, path: Option<&Path>) -> ExportFormat {
    if let Some(format) = explicit {
        return format;
    }

    path.and_then(infer_export_format)
        .unwrap_or(ExportFormat::Png)
}

fn infer_export_format(path: &Path) -> Option<ExportFormat> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "json" => Some(ExportFormat::Json),
        "ansi" | "ans" => Some(ExportFormat::Ansi),
        "png" => Some(ExportFormat::Png),
        "svg" => Some(ExportFormat::Svg),
        _ => None,
    }
}

fn write_payload(path: Option<PathBuf>, payload: &[u8], stdout: &mut dyn Write) -> Result<()> {
    match path {
        Some(path) => {
            let mut out = File::create(&path)
                .with_context(|| format!("creating export target {}", path.display()))?;
            out.write_all(payload)
                .with_context(|| format!("writing export output to {}", path.display()))?;
        }
        None => {
            stdout.write_all(payload)?;
        }
    }

    Ok(())
}

fn build_json_payload(
    source: &InputSource,
    profile: &InputProfile,
    x: Option<&str>,
    y: Option<&str>,
    group: Option<&str>,
    kind: PlotKind,
) -> Result<Vec<u8>> {
    let metadata = match profile.content {
        ContentKind::Png | ContentKind::Jpeg | ContentKind::Gif | ContentKind::Webp => {
            let info = raster::read_metadata(source).context("reading raster metadata")?;
            json!({
                "content_type": "raster",
                "dimensions": {
                    "available": info.dimensions.is_some(),
                    "width": info.dimensions.map(|value| value.width),
                    "height": info.dimensions.map(|value| value.height),
                },
                "color": info.color,
                "frames": info.frames,
            })
        }
        ContentKind::Svg => {
            let info = svg::read_metadata(source).context("reading svg metadata")?;
            json!({
                "content_type": "vector",
                "viewport": info.viewport.map(|value| json!({
                    "width": value.width,
                    "height": value.height,
                })),
                "available": info.viewport.is_some(),
            })
        }
        ContentKind::Csv | ContentKind::Tsv | ContentKind::Jsonl => {
            load_plot_payload_for(source, profile.content, x, y, group, kind)?
        }
    };

    let payload = json!({
        "content": format!("{:?}", profile.content),
        "shape": format!("{:?}", profile.shape),
        "load": format!("{:?}", profile.load),
        "render": format!("{:?}", profile.render),
        "export": format!("{:?}", profile.export),
        "plot_kind": profile
            .plot_kind
            .map(|item| format!("{:?}", item))
            .unwrap_or_else(|| "none".to_owned()),
        "metadata": metadata,
    });

    Ok(serde_json::to_string_pretty(&payload)
        .context("serializing json metadata")?
        .into_bytes())
}

fn load_plot_payload_for(
    source: &InputSource,
    content: ContentKind,
    x: Option<&str>,
    y: Option<&str>,
    group: Option<&str>,
    kind: PlotKind,
) -> Result<Value> {
    let scene = load_plot_scene(source, content, x, y, group, kind)?;
    if let Some(scene) = scene {
        Ok(plot_summary(scene, kind))
    } else {
        Ok(json!({
            "plot_scene": {
                "loaded": false,
                "reason": kind.missing_fields_reason(),
            }
        }))
    }
}

fn build_png_payload(
    source: &InputSource,
    profile: &InputProfile,
    x: Option<&str>,
    y: Option<&str>,
    group: Option<&str>,
    kind: PlotKind,
) -> Result<Vec<u8>> {
    match profile.content {
        ContentKind::Png | ContentKind::Jpeg | ContentKind::Gif | ContentKind::Webp => {
            let image = ImageReader::open(source.path())
                .with_context(|| format!("opening {} for png export", source.label()))?
                .with_guessed_format()
                .context("detecting raster format")?
                .decode()
                .context("decoding raster image")?;
            protocols::encode_png(&image).context("encoding png export payload")
        }
        ContentKind::Csv | ContentKind::Tsv | ContentKind::Jsonl => {
            let scene = load_plot_scene(source, profile.content, x, y, group, kind)?
                .ok_or_else(|| anyhow!("{}", kind.missing_fields_reason()))?;
            let image = protocols::plot::render_plot(&scene, kind)
                .context("rendering plot to png payload")?;
            protocols::encode_png(&image).context("encoding png payload from plot scene")
        }
        ContentKind::Svg => {
            bail!(
                "--output-format png is not supported for vector input in this phase; use --output-format svg instead"
            )
        }
    }
}

fn build_svg_payload(
    source: &InputSource,
    profile: &InputProfile,
    x: Option<&str>,
    y: Option<&str>,
    group: Option<&str>,
    kind: PlotKind,
) -> Result<Vec<u8>> {
    match profile.content {
        ContentKind::Svg => {
            let mut source_file = source
                .open()
                .with_context(|| format!("opening {} for svg export", source.label()))?;
            let mut payload = String::new();
            source_file
                .read_to_string(&mut payload)
                .with_context(|| format!("reading svg source {}", source.label()))?;
            Ok(payload.into_bytes())
        }
        ContentKind::Csv | ContentKind::Tsv | ContentKind::Jsonl => {
            let scene = load_plot_scene(source, profile.content, x, y, group, kind)?
                .ok_or_else(|| anyhow!("{}", kind.missing_fields_reason()))?;
            Ok(protocols::plot::render_svg(&scene, kind)
                .context("rendering plot to svg payload")?
                .into_bytes())
        }
        ContentKind::Png | ContentKind::Jpeg | ContentKind::Webp | ContentKind::Gif => {
            bail!("--output-format svg is not supported for raster inputs in this phase")
        }
    }
}

fn load_plot_scene(
    source: &InputSource,
    content: ContentKind,
    x: Option<&str>,
    y: Option<&str>,
    group: Option<&str>,
    kind: PlotKind,
) -> Result<Option<PlotScene>> {
    if x.is_none() || (kind.requires_y() && y.is_none()) {
        return Ok(None);
    }

    let scene = match content {
        ContentKind::Csv if kind == PlotKind::Histogram => {
            table::load_histogram_scene(source, x, group, b',')
                .context("loading CSV histogram scene")?
        }
        ContentKind::Tsv if kind == PlotKind::Histogram => {
            table::load_histogram_scene(source, x, group, b'\t')
                .context("loading TSV histogram scene")?
        }
        ContentKind::Jsonl if kind == PlotKind::Histogram => {
            stream::load_histogram_scene(source, x, group)
                .context("loading JSONL histogram scene")?
        }
        ContentKind::Csv => {
            table::load_scene(source, x, y, group, b',').context("loading CSV plot scene")?
        }
        ContentKind::Tsv => {
            table::load_scene(source, x, y, group, b'\t').context("loading TSV plot scene")?
        }
        ContentKind::Jsonl => {
            stream::load_scene(source, x, y, group).context("loading JSONL plot scene")?
        }
        _ => unreachable!(),
    };

    Ok(Some(scene))
}

fn plot_summary(scene: PlotScene, kind: PlotKind) -> Value {
    let bounds = kind.render_bounds(&scene).unwrap_or(PlotBounds {
        x_min: 0.0,
        x_max: 0.0,
        y_min: 0.0,
        y_max: 0.0,
    });

    let mut series_summaries = Vec::with_capacity(scene.series.len());
    for series in &scene.series {
        series_summaries.push(json!({
            "name": series.name,
            "points": series.points.len(),
        }));
    }

    json!({
        "plot_scene": {
            "loaded": true,
            "kind": format!("{:?}", kind),
            "series_count": scene.series.len(),
            "point_count": scene.total_points(),
            "bounds": {
                "x_min": bounds.x_min,
                "x_max": bounds.x_max,
                "y_min": bounds.y_min,
                "y_max": bounds.y_max,
            },
            "series": series_summaries,
        }
    })
}

fn build_ansi_payload(
    source: &InputSource,
    profile: &InputProfile,
    x: Option<&str>,
    y: Option<&str>,
    group: Option<&str>,
    kind: PlotKind,
) -> Result<Vec<u8>> {
    match profile.content {
        ContentKind::Png | ContentKind::Jpeg | ContentKind::Gif | ContentKind::Webp => {
            let image = ImageReader::open(source.path())
                .with_context(|| format!("opening {} for ansi render", source.label()))?
                .with_guessed_format()
                .context("detecting raster format")?
                .decode()
                .context("decoding raster image")?;
            Ok(blocks::render_raster(&image)?.into_bytes())
        }
        ContentKind::Csv | ContentKind::Tsv | ContentKind::Jsonl => {
            let scene = load_plot_scene(source, profile.content, x, y, group, kind)?
                .ok_or_else(|| anyhow!("{}", kind.missing_fields_reason()))?;
            Ok(blocks::render_plot(&scene, kind)?.into_bytes())
        }
        ContentKind::Svg => {
            bail!("ansi export for vector content is not implemented in this phase")
        }
    }
}
