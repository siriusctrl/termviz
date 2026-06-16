use anyhow::Result;
use image::{RgbaImage, imageops};

use crate::{
    plot::{
        PlotKind,
        model::{PlotBounds, PlotScene},
    },
    render::Protocol,
    tui::TerminalSize,
};

use super::{
    cache::{
        CachedPlotFrame, PlotFrameCacheKey, PrefetchRequest, prepared_plot_frame_from_rgba,
        render_plot_frame_for_key,
    },
    chrome::{pixel_protocol_target_size, plot_protocol_layout},
};

const PAN_ATLAS_SPAN_MULTIPLIER: f64 = 2.0;

#[derive(Debug)]
struct PanAtlas {
    key: PanAtlasKey,
    bounds: PlotBounds,
    target_width: u32,
    target_height: u32,
    marks: RgbaImage,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PanAtlasKey {
    kind: PlotKind,
    protocol: Protocol,
    size: TerminalSize,
    span_x: f64,
    span_y: f64,
}

pub(super) fn render_pan_prefetch_frames(
    scene: &PlotScene,
    keys: &[PrefetchRequest],
) -> Vec<(PlotFrameCacheKey, Result<CachedPlotFrame, String>)> {
    let Some(first) = keys.first().copied() else {
        return Vec::new();
    };
    let atlas = build_pan_atlas(scene, first.key);
    keys.iter()
        .copied()
        .map(|request| {
            let key = request.key;
            let frame = atlas
                .as_ref()
                .ok()
                .and_then(|atlas| render_key_from_pan_atlas(scene, atlas, request).transpose())
                .unwrap_or_else(|| {
                    render_plot_frame_for_key(
                        scene,
                        request.key,
                        Some(request.image_id),
                        request.transmit_priority,
                    )
                })
                .map_err(|error| error.to_string());
            (key, frame)
        })
        .collect()
}

fn build_pan_atlas(scene: &PlotScene, key: PlotFrameCacheKey) -> Result<PanAtlas> {
    let layout = plot_protocol_layout(key.size);
    let (target_width, target_height) = pixel_protocol_target_size(
        key.protocol,
        u32::from(layout.image_cols),
        u32::from(layout.image_rows),
    );
    let span_x = (key.visible.x_max - key.visible.x_min).abs();
    let span_y = (key.visible.y_max - key.visible.y_min).abs();
    if span_x <= f64::EPSILON || span_y <= f64::EPSILON {
        anyhow::bail!("plot pan atlas requires non-empty spans");
    }
    let margin_x = span_x * (PAN_ATLAS_SPAN_MULTIPLIER - 1.0) / 2.0;
    let margin_y = span_y * (PAN_ATLAS_SPAN_MULTIPLIER - 1.0) / 2.0;
    let bounds = PlotBounds {
        x_min: key.visible.x_min - margin_x,
        x_max: key.visible.x_max + margin_x,
        y_min: key.visible.y_min - margin_y,
        y_max: key.visible.y_max + margin_y,
    };
    let marks = crate::render::protocols::plot::render_interactive_plot_body_marks_rgba_for_size(
        scene,
        key.kind,
        bounds,
        target_width.saturating_mul(PAN_ATLAS_SPAN_MULTIPLIER as u32),
        target_height.saturating_mul(PAN_ATLAS_SPAN_MULTIPLIER as u32),
    )?;
    Ok(PanAtlas {
        key: PanAtlasKey {
            kind: key.kind,
            protocol: key.protocol,
            size: key.size,
            span_x,
            span_y,
        },
        bounds,
        target_width,
        target_height,
        marks,
    })
}

fn render_key_from_pan_atlas(
    scene: &PlotScene,
    atlas: &PanAtlas,
    request: PrefetchRequest,
) -> Result<Option<CachedPlotFrame>> {
    let key = request.key;
    if !atlas.can_render(key.kind, key.protocol, key.size, key.visible) {
        return Ok(None);
    }
    let mut base = crate::render::protocols::plot::render_interactive_plot_body_base_rgba_for_size(
        scene,
        key.visible,
        atlas.target_width,
        atlas.target_height,
    )?;
    let crop = atlas.crop_marks(key.visible);
    imageops::overlay(&mut base, &crop, 0, 0);
    Ok(Some(prepared_plot_frame_from_rgba(
        key,
        &base,
        Some(request.image_id),
        request.transmit_priority,
    )?))
}

impl PanAtlas {
    fn can_render(
        &self,
        kind: PlotKind,
        protocol: Protocol,
        size: TerminalSize,
        visible: PlotBounds,
    ) -> bool {
        self.key.kind == kind
            && self.key.protocol == protocol
            && self.key.size == size
            && spans_close(self.key.span_x, visible.x_max - visible.x_min)
            && spans_close(self.key.span_y, visible.y_max - visible.y_min)
            && visible.x_min >= self.bounds.x_min
            && visible.x_max <= self.bounds.x_max
            && visible.y_min >= self.bounds.y_min
            && visible.y_max <= self.bounds.y_max
    }

    fn crop_marks(&self, visible: PlotBounds) -> RgbaImage {
        let atlas_width = self.marks.width();
        let atlas_height = self.marks.height();
        let span_x = (self.bounds.x_max - self.bounds.x_min)
            .abs()
            .max(f64::EPSILON);
        let span_y = (self.bounds.y_max - self.bounds.y_min)
            .abs()
            .max(f64::EPSILON);
        let max_x = atlas_width.saturating_sub(self.target_width);
        let max_y = atlas_height.saturating_sub(self.target_height);
        let crop_x = (((visible.x_min - self.bounds.x_min) / span_x) * f64::from(atlas_width))
            .round()
            .clamp(0.0, f64::from(max_x)) as u32;
        let crop_y = (((self.bounds.y_max - visible.y_max) / span_y) * f64::from(atlas_height))
            .round()
            .clamp(0.0, f64::from(max_y)) as u32;
        imageops::crop_imm(
            &self.marks,
            crop_x,
            crop_y,
            self.target_width,
            self.target_height,
        )
        .to_image()
    }
}

fn spans_close(first: f64, second: f64) -> bool {
    let first = first.abs();
    let second = second.abs();
    (first - second).abs() <= first.max(second).max(1.0) * 1e-9
}
