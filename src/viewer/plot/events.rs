use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseEventKind};

use crate::tui::{TerminalSession, TerminalSize};

use super::{
    hover::PlotHoverCell,
    state::{PlotNavAction, PlotViewState},
};

const MAX_PENDING_EVENTS_PER_FRAME: usize = 64;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct PlotEventOutcome {
    pub(super) dirty: bool,
    pub(super) hover_dirty: bool,
    pub(super) quit: bool,
    pub(super) resized: bool,
    pub(super) action: Option<PlotNavAction>,
}

pub(super) fn drain_pending_plot_events(
    session: &mut TerminalSession,
    size: &mut TerminalSize,
    state: &mut PlotViewState,
    show_overlay: &mut bool,
    hover_cell: &mut Option<PlotHoverCell>,
) -> Result<PlotEventOutcome> {
    let mut outcome = PlotEventOutcome::default();
    for _ in 0..MAX_PENDING_EVENTS_PER_FRAME {
        let Some(event) = session.read_pending_event()? else {
            break;
        };
        let next = handle_plot_event(event, size, state, show_overlay, hover_cell);
        outcome.dirty |= next.dirty;
        outcome.hover_dirty |= next.hover_dirty;
        outcome.resized |= next.resized;
        outcome.action = next.action.or(outcome.action);
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
    hover_cell: &mut Option<PlotHoverCell>,
) -> PlotEventOutcome {
    match event {
        Event::Resize(cols, rows) => {
            let previous_size = *size;
            size.width = cols.max(1);
            size.height = rows.max(1);
            PlotEventOutcome {
                dirty: *size != previous_size,
                hover_dirty: false,
                resized: *size != previous_size,
                quit: false,
                action: None,
            }
        }
        Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
            let previous_state = *state;
            let previous_overlay = *show_overlay;
            let mut action = None;
            match key_event.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    return PlotEventOutcome {
                        dirty: false,
                        hover_dirty: false,
                        quit: true,
                        resized: false,
                        action: None,
                    };
                }
                KeyCode::Left => action = Some(PlotNavAction::PanLeft),
                KeyCode::Right => action = Some(PlotNavAction::PanRight),
                KeyCode::Up => action = Some(PlotNavAction::PanUp),
                KeyCode::Down => action = Some(PlotNavAction::PanDown),
                KeyCode::Char('+') | KeyCode::Char('=') => action = Some(PlotNavAction::ZoomIn),
                KeyCode::Char('-') => action = Some(PlotNavAction::ZoomOut),
                KeyCode::Char('0') => action = Some(PlotNavAction::Reset),
                KeyCode::Char('m') | KeyCode::Char('M') => {
                    *show_overlay = !*show_overlay;
                }
                _ => {}
            }
            if let Some(action) = action {
                state.apply_nav_action(action);
            }
            PlotEventOutcome {
                dirty: *state != previous_state || *show_overlay != previous_overlay,
                hover_dirty: false,
                quit: false,
                resized: false,
                action,
            }
        }
        Event::Mouse(mouse_event)
            if matches!(
                mouse_event.kind,
                MouseEventKind::Moved
                    | MouseEventKind::Drag(_)
                    | MouseEventKind::Down(_)
                    | MouseEventKind::Up(_)
            ) =>
        {
            let next = Some(PlotHoverCell {
                col: mouse_event.column,
                row: mouse_event.row,
            });
            let changed = *hover_cell != next;
            *hover_cell = next;
            PlotEventOutcome {
                dirty: false,
                hover_dirty: changed,
                quit: false,
                resized: false,
                action: None,
            }
        }
        _ => PlotEventOutcome::default(),
    }
}
