use anyhow::{Result, bail};

use crate::{
    input::{
        InputSource,
        sniff::{ContentSample, ExtensionHint, extension_hint},
    },
    plot::PlotKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InputProfile {
    pub(crate) content: ContentKind,
    pub(crate) shape: ContentShape,
    pub(crate) load: LoadStrategy,
    pub(crate) render: RenderStrategy,
    pub(crate) export: ExportPolicy,
    pub(crate) plot_kind: Option<PlotKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentKind {
    Png,
    Jpeg,
    Gif,
    Webp,
    Svg,
    Csv,
    Tsv,
    Jsonl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
    Svg,
    Csv,
    Tsv,
    Jsonl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentShape {
    RasterImage,
    VectorImage,
    AnimatedFrames,
    DataTable,
    DataStream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoadStrategy {
    MetadataFirst,
    TiledRaster,
    RasterizeVector,
    BoundedTableSample,
    BoundedRecordSample,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderStrategy {
    TerminalImage,
    TerminalPlot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExportPolicy {
    ExplicitOnly,
}

impl InputProfile {
    pub(crate) fn resolve(
        source: &InputSource,
        plot_kind: PlotKind,
        input_format: Option<InputFormat>,
    ) -> Result<Self> {
        if let Some(input_format) = input_format {
            return Ok(profile_from_input_format(input_format, plot_kind));
        }

        if let Some(profile) = profile_from_extension(extension_hint(source), plot_kind) {
            return Ok(profile);
        }

        let sample = ContentSample::read(source)?;
        if let Some(profile) = profile_from_sample(sample, plot_kind) {
            return Ok(profile);
        }

        bail!(
            "could not determine input type for {}; use a known image or data extension, or force it with --input-format",
            source.label()
        )
    }

    pub(crate) fn inspect_text(self) -> String {
        format!(
            "content={:?}\nshape={:?}\nload={:?}\nrender={:?}\nexport={:?}\nplot_kind={}",
            self.content,
            self.shape,
            self.load,
            self.render,
            self.export,
            self.plot_kind
                .map(|kind| format!("{kind:?}"))
                .unwrap_or_else(|| "none".to_owned())
        )
    }
}

fn profile_from_input_format(format: InputFormat, plot_kind: PlotKind) -> InputProfile {
    match format {
        InputFormat::Png => raster(ContentKind::Png),
        InputFormat::Jpeg => raster(ContentKind::Jpeg),
        InputFormat::Gif => InputProfile {
            content: ContentKind::Gif,
            shape: ContentShape::AnimatedFrames,
            load: LoadStrategy::MetadataFirst,
            render: RenderStrategy::TerminalImage,
            export: ExportPolicy::ExplicitOnly,
            plot_kind: None,
        },
        InputFormat::Webp => raster(ContentKind::Webp),
        InputFormat::Svg => InputProfile {
            content: ContentKind::Svg,
            shape: ContentShape::VectorImage,
            load: LoadStrategy::RasterizeVector,
            render: RenderStrategy::TerminalImage,
            export: ExportPolicy::ExplicitOnly,
            plot_kind: None,
        },
        InputFormat::Csv => table(ContentKind::Csv, plot_kind),
        InputFormat::Tsv => table(ContentKind::Tsv, plot_kind),
        InputFormat::Jsonl => InputProfile {
            content: ContentKind::Jsonl,
            shape: ContentShape::DataStream,
            load: LoadStrategy::BoundedRecordSample,
            render: RenderStrategy::TerminalPlot,
            export: ExportPolicy::ExplicitOnly,
            plot_kind: Some(plot_kind),
        },
    }
}

fn profile_from_extension(hint: ExtensionHint, plot_kind: PlotKind) -> Option<InputProfile> {
    Some(match hint {
        ExtensionHint::Png => raster(ContentKind::Png),
        ExtensionHint::Jpeg => raster(ContentKind::Jpeg),
        ExtensionHint::Gif => InputProfile {
            content: ContentKind::Gif,
            shape: ContentShape::AnimatedFrames,
            load: LoadStrategy::MetadataFirst,
            render: RenderStrategy::TerminalImage,
            export: ExportPolicy::ExplicitOnly,
            plot_kind: None,
        },
        ExtensionHint::Webp => raster(ContentKind::Webp),
        ExtensionHint::Svg => InputProfile {
            content: ContentKind::Svg,
            shape: ContentShape::VectorImage,
            load: LoadStrategy::RasterizeVector,
            render: RenderStrategy::TerminalImage,
            export: ExportPolicy::ExplicitOnly,
            plot_kind: None,
        },
        ExtensionHint::Csv => table(ContentKind::Csv, plot_kind),
        ExtensionHint::Tsv => table(ContentKind::Tsv, plot_kind),
        ExtensionHint::Jsonl => InputProfile {
            content: ContentKind::Jsonl,
            shape: ContentShape::DataStream,
            load: LoadStrategy::BoundedRecordSample,
            render: RenderStrategy::TerminalPlot,
            export: ExportPolicy::ExplicitOnly,
            plot_kind: Some(plot_kind),
        },
        ExtensionHint::Unknown => return None,
    })
}

fn profile_from_sample(sample: ContentSample, plot_kind: PlotKind) -> Option<InputProfile> {
    if sample.looks_like_svg || sample.first_non_ws == Some(b'<') {
        return profile_from_extension(ExtensionHint::Svg, plot_kind);
    }
    if sample.lines > 0 && sample.jsonl_lines == sample.lines {
        return profile_from_extension(ExtensionHint::Jsonl, plot_kind);
    }
    if sample.lines > 0 && sample.comma_lines * 2 >= sample.lines {
        return profile_from_extension(ExtensionHint::Csv, plot_kind);
    }
    if sample.lines > 0 && sample.tab_lines * 2 >= sample.lines {
        return profile_from_extension(ExtensionHint::Tsv, plot_kind);
    }
    None
}

fn raster(content: ContentKind) -> InputProfile {
    InputProfile {
        content,
        shape: ContentShape::RasterImage,
        load: LoadStrategy::MetadataFirst,
        render: RenderStrategy::TerminalImage,
        export: ExportPolicy::ExplicitOnly,
        plot_kind: None,
    }
}

fn table(content: ContentKind, plot_kind: PlotKind) -> InputProfile {
    InputProfile {
        content,
        shape: ContentShape::DataTable,
        load: LoadStrategy::BoundedTableSample,
        render: RenderStrategy::TerminalPlot,
        export: ExportPolicy::ExplicitOnly,
        plot_kind: Some(plot_kind),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn csv_extension_resolves_to_table_plot() {
        let mut file = NamedTempFile::with_suffix(".csv").unwrap();
        writeln!(file, "time,latency").unwrap();
        writeln!(file, "1,20").unwrap();
        let source = InputSource::from_path(file.path().to_path_buf()).unwrap();

        let profile = InputProfile::resolve(&source, PlotKind::Line, None).unwrap();

        assert_eq!(profile.content, ContentKind::Csv);
        assert_eq!(profile.shape, ContentShape::DataTable);
        assert_eq!(profile.render, RenderStrategy::TerminalPlot);
    }

    #[test]
    fn png_extension_resolves_to_raster_image() {
        let mut file = NamedTempFile::with_suffix(".png").unwrap();
        file.write_all(b"not decoded yet").unwrap();
        let source = InputSource::from_path(file.path().to_path_buf()).unwrap();

        let profile = InputProfile::resolve(&source, PlotKind::Line, None).unwrap();

        assert_eq!(profile.content, ContentKind::Png);
        assert_eq!(profile.shape, ContentShape::RasterImage);
        assert_eq!(profile.render, RenderStrategy::TerminalImage);
    }
}
