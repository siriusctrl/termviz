use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEventKind};

use crate::tui::{TerminalSession, TerminalSize};

use super::state::PlotViewState;

const MAX_PENDING_EVENTS_PER_FRAME: usize = 64;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct PlotEventOutcome {
    pub(super) dirty: bool,
    pub(super) quit: bool,
    pub(super) resized: bool,
}

pub(super) fn drain_pending_plot_events(
    session: &mut TerminalSession,
    size: &mut TerminalSize,
    state: &mut PlotViewState,
    show_overlay: &mut bool,
) -> Result<PlotEventOutcome> {
    let mut outcome = PlotEventOutcome::default();
    for _ in 0..MAX_PENDING_EVENTS_PER_FRAME {
        let Some(event) = session.read_pending_event()? else {
            break;
        };
        let next = handle_plot_event(event, size, state, show_overlay);
        outcome.dirty |= next.dirty;
        outcome.resized |= next.resized;
        if next.quit {
            outcome.quit = true;
            break;
        }
    }
    Ok(outcome)
}

pub(super) fn handle_plot_event(
    event: Event,
    size: &mut TerminalSize,
    state: &mut PlotViewState,
    show_overlay: &mut bool,
) -> PlotEventOutcome {
    match event {
        Event::Resize(cols, rows) => {
            let previous_size = *size;
            size.width = cols.max(1);
            size.height = rows.max(1);
            PlotEventOutcome {
                dirty: *size != previous_size,
                resized: *size != previous_size,
                quit: false,
            }
        }
        Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
            let previous_state = *state;
            let previous_overlay = *show_overlay;
            match key_event.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    return PlotEventOutcome {
                        dirty: false,
                        quit: true,
                        resized: false,
                    };
                }
                KeyCode::Left => state.pan_left(),
                KeyCode::Right => state.pan_right(),
                KeyCode::Up => state.pan_up(),
                KeyCode::Down => state.pan_down(),
                KeyCode::Char('+') | KeyCode::Char('=') => state.zoom_in(),
                KeyCode::Char('-') => state.zoom_out(),
                KeyCode::Char('0') => state.reset(),
                KeyCode::Char('m') | KeyCode::Char('M') => {
                    *show_overlay = !*show_overlay;
                }
                _ => {}
            }
            PlotEventOutcome {
                dirty: *state != previous_state || *show_overlay != previous_overlay,
                quit: false,
                resized: false,
            }
        }
        _ => PlotEventOutcome::default(),
    }
}
