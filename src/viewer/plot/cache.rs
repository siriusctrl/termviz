use anyhow::{Context, Result};

use crate::{
    plot::{
        PlotKind,
        model::{PlotBounds, PlotScene},
    },
    render::{
        Protocol,
        protocols::{ProtocolRenderContext, blocks, render_plot_rgba_with_fallback},
    },
    tui::TerminalSize,
};

use super::{
    chrome::{pixel_protocol_target_size, plot_protocol_layout},
    state::PlotViewState,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PlotFrameCacheKey {
    kind: PlotKind,
    protocol: Protocol,
    visible: PlotBounds,
    pub(super) size: TerminalSize,
}

#[derive(Debug, Default)]
pub(super) struct PlotFrameCache {
    pub(super) last: Option<CachedPlotFrame>,
}

#[derive(Debug)]
pub(super) struct CachedPlotFrame {
    pub(super) key: PlotFrameCacheKey,
    frame: String,
}

impl PlotFrameCache {
    pub(super) fn get_or_render(
        &mut self,
        scene: &PlotScene,
        kind: PlotKind,
        state: &PlotViewState,
        protocol: Protocol,
        size: TerminalSize,
    ) -> Result<&str> {
        let key = PlotFrameCacheKey {
            kind,
            protocol,
            visible: state.visible,
            size,
        };
        let cache_hit = self.last.as_ref().is_some_and(|cached| cached.key == key);
        if cache_hit {
            return Ok(self
                .last
                .as_ref()
                .map(|cached| cached.frame.as_str())
                .unwrap_or_default());
        }

        let frame = render_plot_frame(scene, kind, state, protocol, size)?;
        self.last = Some(CachedPlotFrame { key, frame });
        Ok(self
            .last
            .as_ref()
            .map(|cached| cached.frame.as_str())
            .unwrap_or_default())
    }
}

pub(super) fn render_plot_frame(
    scene: &PlotScene,
    kind: PlotKind,
    state: &PlotViewState,
    protocol: Protocol,
    size: TerminalSize,
) -> Result<String> {
    let cols = u32::from(size.width);
    let rows = u32::from(size.height.saturating_sub(1)).max(1);
    if cols == 0 || rows == 0 {
        return Ok(String::new());
    }

    if protocol == Protocol::Blocks {
        let drawable_cols = u32::from(size.width.saturating_sub(1).max(1));
        return blocks::render_terminal_plot_for_size(
            scene,
            kind,
            state.visible,
            drawable_cols,
            rows,
        )
        .context("rendering terminal plot frame");
    }

    let layout = plot_protocol_layout(size);
    let target = pixel_protocol_target_size(
        protocol,
        u32::from(layout.image_cols),
        u32::from(layout.image_rows),
    );
    let image = crate::render::protocols::plot::render_interactive_plot_body_rgba_for_size(
        scene,
        kind,
        state.visible,
        target.0,
        target.1,
    )?;
    let context = ProtocolRenderContext::new(protocol);
    Ok(render_plot_rgba_with_fallback(
        context,
        &image,
        u32::from(layout.image_cols),
        u32::from(layout.image_rows),
    ))
}
