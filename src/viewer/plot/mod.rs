use std::borrow::Cow;

use anyhow::{Context, Result, bail};

use crate::{
    input::InputSource,
    plot::model::PlotScene,
    plot::{PlotKind, stream, table},
    profile::{ContentKind, InputProfile},
    render::Protocol,
    tui::{TerminalSession, TerminalSize},
};

mod atlas;
mod cache;
mod chrome;
mod events;
mod hover;
mod state;

use cache::{PlotFrameCache, render_hover_plot_frame};
use chrome::{
    PlotProtocolChromeLines, plot_protocol_chrome, render_plot_overlay, status_line_chrome,
    status_line_text,
};
use events::{drain_pending_plot_events, handle_plot_event};
use hover::{PlotHoverCell, hover_for_cell};
use state::PlotViewState;

#[cfg(test)]
use crate::{
    plot::model::PlotBounds,
    render::protocols::{ProtocolRenderContext, render_plot_rgba_with_fallback},
    tui::{ChromeLine, ChromeRole, ChromeSegment},
};
#[cfg(test)]
use cache::render_plot_frame;
#[cfg(test)]
use chrome::{
    CELL_PIXEL_HEIGHT, CELL_PIXEL_WIDTH, MAX_INTERACTIVE_PLOT_PIXELS, pixel_protocol_target_size,
    plot_protocol_layout,
};
#[cfg(test)]
use crossterm::event::Event;
#[cfg(test)]
use state::PlotNavAction;

pub(crate) fn run(
    source: InputSource,
    profile: InputProfile,
    protocol: Protocol,
    request: PlotRequest,
) -> Result<()> {
    let scene = load_scene(
        &source,
        &profile,
        request.x.as_deref(),
        request.y.as_deref(),
        request.group.as_deref(),
        request.kind,
    )?;
    let bounds = request
        .kind
        .render_bounds(&scene)
        .context("plot scene is empty")?;
    let mut state = PlotViewState::new(bounds);
    let protocol = resolve_plot_protocol(protocol);

    let mut session = TerminalSession::start()?;
    let mut size = session.size().context("reading initial terminal size")?;
    let mut show_overlay = false;
    let mut dirty = true;
    let mut frame_cache = PlotFrameCache::default();
    let mut last_protocol_chrome_size = None;
    let mut recent_action = None;
    let mut hover_cell: Option<PlotHoverCell> = None;

    loop {
        if dirty {
            let outcome = drain_pending_plot_events(
                &mut session,
                &mut size,
                &mut state,
                &mut show_overlay,
                &mut hover_cell,
            )?;
            if outcome.quit {
                break;
            }
            let prefetch_action = outcome.action.or(recent_action.take());
            if outcome.resized {
                size = session
                    .size()
                    .context("reading terminal size after resize")?;
            }
            if let Ok(actual_size) = session.size()
                && actual_size != size
            {
                size = actual_size;
            }

            let hover = current_hover(&scene, &state, size, show_overlay, hover_cell);
            let status_text = status_line_text(size, show_overlay);

            let frame: Cow<'_, str> = if show_overlay {
                Cow::Owned(render_plot_overlay(&scene))
            } else if let Some(hover) = hover.as_ref() {
                render_hover_plot_frame(
                    &mut frame_cache,
                    &scene,
                    request.kind,
                    &state,
                    protocol,
                    size,
                    hover,
                )?
            } else {
                frame_cache.get_or_render(&scene, request.kind, &state, protocol, size)?
            };

            if protocol == Protocol::Blocks || show_overlay {
                session.draw_frame_with_readout(
                    frame.as_ref(),
                    &chrome::readout_line_chrome(hover.as_ref()),
                    &status_text,
                )?;
                last_protocol_chrome_size = None;
            } else {
                let repaint_static_chrome = last_protocol_chrome_size != Some(size);
                let chrome = plot_protocol_chrome(
                    frame.as_ref(),
                    &scene,
                    &state,
                    protocol,
                    size,
                    PlotProtocolChromeLines {
                        readout: chrome::readout_line_chrome(hover.as_ref()),
                        status: status_line_chrome(show_overlay),
                    },
                    repaint_static_chrome,
                );
                session.draw_plot_protocol_frame(chrome)?;
                last_protocol_chrome_size = Some(size);
            }
            if !show_overlay {
                frame_cache.prefetch_neighbors(
                    &scene,
                    request.kind,
                    &state,
                    protocol,
                    size,
                    prefetch_action,
                );
            }
            dirty = false;
        }

        if let Some(event) = session.read_event()? {
            let outcome = handle_plot_event(
                event,
                &mut size,
                &mut state,
                &mut show_overlay,
                &mut hover_cell,
            );
            if outcome.quit {
                break;
            }
            recent_action = outcome.action.or(recent_action);
            dirty = dirty || outcome.dirty;
            dirty = dirty || outcome.hover_dirty;
        } else if protocol == Protocol::Kitty && !show_overlay {
            let payloads = frame_cache.drain_transmit_payloads(1);
            session.write_protocol_payloads(&payloads)?;
        }
    }

    Ok(())
}

fn current_hover(
    scene: &PlotScene,
    state: &PlotViewState,
    size: TerminalSize,
    show_overlay: bool,
    hover_cell: Option<PlotHoverCell>,
) -> Option<hover::PlotHover> {
    if show_overlay {
        return None;
    }
    hover_cell.and_then(|cell| hover_for_cell(scene, state, size, cell))
}

fn resolve_plot_protocol(protocol: Protocol) -> Protocol {
    match protocol {
        Protocol::Auto => crate::render::terminal::detect(Protocol::Auto).preferred,
        Protocol::Blocks | Protocol::Kitty => protocol,
    }
}

fn load_scene(
    source: &InputSource,
    profile: &InputProfile,
    x: Option<&str>,
    y: Option<&str>,
    group: Option<&str>,
    kind: PlotKind,
) -> Result<PlotScene> {
    match profile.content {
        ContentKind::Csv if kind == PlotKind::Histogram => {
            table::load_histogram_scene(source, x, group, b',')
                .context("loading histogram scene from CSV data")
        }
        ContentKind::Tsv if kind == PlotKind::Histogram => {
            table::load_histogram_scene(source, x, group, b'\t')
                .context("loading histogram scene from TSV data")
        }
        ContentKind::Jsonl if kind == PlotKind::Histogram => {
            stream::load_histogram_scene(source, x, group)
                .context("loading histogram scene from jsonl data")
        }
        ContentKind::Csv => {
            table::load_scene(source, x, y, group, b',').context("loading plot scene from CSV data")
        }
        ContentKind::Tsv => table::load_scene(source, x, y, group, b'\t')
            .context("loading plot scene from TSV data"),
        ContentKind::Jsonl => {
            stream::load_scene(source, x, y, group).context("loading plot scene from jsonl data")
        }
        _ => bail!("plot viewer only supports csv, tsv, and jsonl inputs"),
    }
}

pub(crate) struct PlotRequest {
    pub(crate) x: Option<String>,
    pub(crate) y: Option<String>,
    pub(crate) group: Option<String>,
    pub(crate) kind: PlotKind,
}

#[cfg(test)]
mod tests;
