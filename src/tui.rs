use std::{
    io::{self, Stdout, Write},
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute, queue,
    style::Print,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use unicode_width::UnicodeWidthChar;

pub(crate) mod layout;
pub(crate) mod palette;

const EVENT_POLL_TIMEOUT: Duration = Duration::from_millis(25);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalSize {
    pub(crate) width: u16,
    pub(crate) height: u16,
}

impl TerminalSize {
    pub(crate) fn content_height(&self) -> u16 {
        self.height.saturating_sub(1)
    }
}

pub(crate) struct TerminalSession {
    stdout: Stdout,
    active: bool,
}

impl TerminalSession {
    pub(crate) fn start() -> Result<Self> {
        let mut stdout = io::stdout();
        terminal::enable_raw_mode().context("enable terminal raw mode")?;
        if let Err(err) = execute!(
            &mut stdout,
            EnterAlternateScreen,
            Hide,
            Clear(ClearType::All),
            EnableMouseCapture
        )
        .context("enter alternate screen and prepare frame")
        {
            let _ = execute!(&mut stdout, DisableMouseCapture, Show, LeaveAlternateScreen);
            let _ = terminal::disable_raw_mode();
            return Err(err);
        }

        Ok(Self {
            stdout,
            active: true,
        })
    }

    pub(crate) fn read_event(&mut self) -> Result<Option<Event>> {
        if event::poll(EVENT_POLL_TIMEOUT).context("polling terminal events")? {
            let event = event::read().context("reading terminal event")?;
            return Ok(Some(event));
        }
        Ok(None)
    }

    pub(crate) fn read_pending_event(&mut self) -> Result<Option<Event>> {
        if event::poll(Duration::ZERO).context("polling pending terminal events")? {
            let event = event::read().context("reading pending terminal event")?;
            return Ok(Some(event));
        }
        Ok(None)
    }

    pub(crate) fn size(&mut self) -> Result<TerminalSize> {
        let (width, height) = terminal::size().context("reading terminal size")?;
        Ok(TerminalSize {
            width: width.max(1),
            height: height.max(1),
        })
    }

    pub(crate) fn draw_frame(&mut self, content: &str, status: &str) -> Result<()> {
        let size = self.size()?;
        queue!(
            self.stdout,
            MoveTo(0, 0),
            Clear(ClearType::All),
            Print(content_to_lines(content, size))
        )
        .context("drawing terminal frame")?;
        self.stdout.flush().context("flushing terminal frame")?;
        if size.height > 0 {
            self.draw_status_line(size, status)?;
        }
        self.stdout.flush().context("flushing status line")?;
        Ok(())
    }

    pub(crate) fn draw_protocol_frame(&mut self, payload: &str, status: &str) -> Result<()> {
        let size = self.size()?;
        queue!(self.stdout, MoveTo(0, 0), Print(payload)).context("drawing protocol payload")?;
        self.stdout.flush().context("flushing protocol payload")?;
        if size.height > 0 {
            self.draw_status_line(size, status)?;
        }
        self.stdout.flush().context("flushing status line")?;
        Ok(())
    }

    pub(crate) fn draw_status_line(&mut self, size: TerminalSize, text: &str) -> Result<()> {
        let status_row = size.height.saturating_sub(1);
        let content_width = size.width.max(1);
        queue!(
            self.stdout,
            MoveTo(0, status_row),
            Clear(ClearType::CurrentLine),
            Print(trim_to_width(text, usize::from(content_width)))
        )
        .context("drawing status line")?;
        Ok(())
    }

    pub(crate) fn stop(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        let restore = execute!(
            self.stdout,
            DisableMouseCapture,
            Show,
            Clear(ClearType::All),
            LeaveAlternateScreen
        )
        .map_err(|error| anyhow!("restore terminal screen state: {error}"));
        let raw_mode = terminal::disable_raw_mode()
            .map_err(|error| anyhow!("disable terminal raw mode: {error}"));

        self.active = false;

        match (restore, raw_mode) {
            (Ok(_), Ok(_)) => {}
            (Err(error), Ok(_)) => return Err(error),
            (Ok(_), Err(error)) => return Err(error),
            (Err(restore_error), Err(raw_error)) => {
                return Err(anyhow!(
                    "failed to restore terminal screen state: {restore_error}; then failed to disable raw mode: {raw_error}"
                ));
            }
        }

        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn content_to_lines(content: &str, size: TerminalSize) -> String {
    let width = usize::from(size.width.max(1));
    let content_rows = usize::from(size.content_height().max(1));
    let mut output = String::new();
    for (row, line) in content.lines().take(content_rows).enumerate() {
        if line.contains('\u{1b}') {
            output.push_str(line);
        } else {
            output.push_str(&trim_to_width(line, width));
        }
        if row + 1 < content_rows || size.height > 1 {
            output.push_str("\r\n");
        }
    }

    let used_rows = content.lines().count().min(content_rows);
    for _ in used_rows..content_rows {
        output.push_str(&" ".repeat(width));
        output.push_str("\r\n");
    }
    output
}

fn trim_to_width(text: &str, width: usize) -> String {
    let mut output = String::new();
    let mut used = 0usize;

    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(1).max(1);
        if used + ch_width > width {
            break;
        }
        used += ch_width;
        output.push(ch);
    }

    if used < width {
        output.push_str(&" ".repeat(width - used));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_content_uses_crlf_for_raw_terminal_mode() {
        let output = content_to_lines(
            "alpha\nbeta",
            TerminalSize {
                width: 8,
                height: 3,
            },
        );

        assert!(output.contains("alpha   \r\nbeta"));
        assert!(!output.contains("alpha   \nbeta"));
    }
}
