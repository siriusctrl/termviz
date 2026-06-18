use std::{
    io::{self, Stdout, Write},
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use unicode_width::UnicodeWidthChar;

mod chrome;
mod plot_frame;

use chrome::{draw_chrome_line, draw_status_chrome, paint_rect, paint_row, print_status_text};
pub(crate) use plot_frame::{
    ChromeLine, ChromeRole, ChromeSegment, DynamicPlotChrome, PlotAxisLabel, PlotImageBody,
    PlotLegendItem, PlotProtocolChrome, PlotProtocolFrame, StaticPlotChrome,
};

const EVENT_POLL_TIMEOUT: Duration = Duration::from_millis(8);
const PLOT_CHROME_BG: Color = Color::Rgb { r: 9, g: 13, b: 15 };
const PLOT_TEXT: Color = Color::Rgb {
    r: 205,
    g: 212,
    b: 209,
};
const PLOT_MUTED_TEXT: Color = Color::Rgb {
    r: 128,
    g: 146,
    b: 146,
};

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
            Print(content_to_lines(
                content,
                size.content_height().max(1),
                size.width
            ))
        )
        .context("drawing terminal frame")?;
        if size.height > 0 {
            self.draw_status_line(size, status)?;
        }
        self.stdout.flush().context("flushing terminal frame")?;
        Ok(())
    }

    pub(crate) fn draw_frame_with_readout(
        &mut self,
        content: &str,
        readout: &ChromeLine,
        status: &str,
    ) -> Result<()> {
        let size = self.size()?;
        let content_rows = size.height.saturating_sub(2).max(1);
        queue!(
            self.stdout,
            MoveTo(0, 0),
            Clear(ClearType::All),
            Print(content_to_lines(content, content_rows, size.width))
        )
        .context("drawing terminal frame with readout")?;
        if size.height > 1 {
            draw_chrome_line(
                &mut self.stdout,
                size.height.saturating_sub(2),
                size.width.max(1),
                PLOT_CHROME_BG,
                Color::Rgb {
                    r: 41,
                    g: 54,
                    b: 57,
                },
                readout,
            )
            .context("drawing terminal frame readout")?;
        }
        if size.height > 0 {
            self.draw_status_line(size, status)?;
        }
        self.stdout
            .flush()
            .context("flushing terminal frame with readout")?;
        Ok(())
    }

    pub(crate) fn draw_protocol_frame(&mut self, payload: &str, status: &str) -> Result<()> {
        let size = self.size()?;
        queue!(self.stdout, MoveTo(0, 0), Print(payload)).context("drawing protocol payload")?;
        if size.height > 0 {
            self.draw_status_line(size, status)?;
        }
        self.stdout.flush().context("flushing protocol frame")?;
        Ok(())
    }

    pub(crate) fn draw_plot_protocol_frame(&mut self, frame: PlotProtocolFrame<'_>) -> Result<()> {
        let size = self.size()?;
        if frame.chrome.static_layer.repaint {
            self.draw_plot_protocol_background(size, &frame)
                .context("drawing plot protocol chrome background")?;
            self.draw_plot_legend(size, &frame)
                .context("drawing plot legend")?;
        }
        self.draw_plot_dynamic_chrome(size, &frame)?;
        self.draw_plot_axis_labels(&frame)?;
        if size.height > 0 {
            self.draw_plot_readout_line(size, &frame)?;
            self.draw_plot_status_line(size, &frame.chrome.dynamic_layer.status)?;
        }
        queue!(
            self.stdout,
            MoveTo(frame.body.col, frame.body.row),
            Print(frame.payload)
        )
        .context("drawing plot protocol payload")?;
        self.stdout
            .flush()
            .context("flushing plot protocol frame")?;
        Ok(())
    }

    pub(crate) fn write_protocol_payloads(&mut self, payloads: &[String]) -> Result<()> {
        if payloads.is_empty() {
            return Ok(());
        }
        for payload in payloads {
            queue!(self.stdout, Print(payload)).context("writing protocol payload")?;
        }
        self.stdout.flush().context("flushing protocol payloads")?;
        Ok(())
    }

    fn draw_plot_protocol_background(
        &mut self,
        size: TerminalSize,
        frame: &PlotProtocolFrame<'_>,
    ) -> Result<()> {
        let header_rows = frame.body.row.min(size.height);
        for row in 0..header_rows {
            paint_row(&mut self.stdout, 0, row, size.width, PLOT_CHROME_BG)
                .context("painting plot header background")?;
        }

        let image_bottom = frame
            .body
            .row
            .saturating_add(frame.body.rows)
            .min(size.height);
        let gutter_rows = image_bottom.saturating_sub(frame.body.row);
        paint_rect(
            &mut self.stdout,
            0,
            frame.body.row,
            frame.body.col,
            gutter_rows,
            PLOT_CHROME_BG,
        )
        .context("painting plot y-axis gutter")?;

        if frame.chrome.static_layer.x_axis_row < size.height {
            paint_row(
                &mut self.stdout,
                0,
                frame.chrome.static_layer.x_axis_row,
                size.width,
                PLOT_CHROME_BG,
            )
            .context("painting plot x-axis row")?;
        }

        if frame.chrome.dynamic_layer.readout_row < size.height.saturating_sub(1) {
            paint_row(
                &mut self.stdout,
                0,
                frame.chrome.dynamic_layer.readout_row,
                size.width,
                PLOT_CHROME_BG,
            )
            .context("painting plot readout row")?;
        }

        Ok(())
    }

    pub(crate) fn draw_status_line(&mut self, size: TerminalSize, text: &str) -> Result<()> {
        let status_row = size.height.saturating_sub(1);
        let content_width = size.width.max(1);
        draw_status_chrome(&mut self.stdout, status_row, content_width, text)
            .context("drawing status line")?;
        Ok(())
    }

    fn draw_plot_header(
        &mut self,
        size: TerminalSize,
        frame: &PlotProtocolFrame<'_>,
    ) -> Result<()> {
        draw_chrome_line(
            &mut self.stdout,
            0,
            size.width,
            PLOT_CHROME_BG,
            Color::Rgb {
                r: 41,
                g: 54,
                b: 57,
            },
            &frame.chrome.dynamic_layer.header,
        )
        .context("drawing plot header")?;
        Ok(())
    }

    fn draw_plot_legend(
        &mut self,
        size: TerminalSize,
        frame: &PlotProtocolFrame<'_>,
    ) -> Result<()> {
        if size.height <= 2 {
            return Ok(());
        }

        queue!(
            self.stdout,
            MoveTo(0, 1),
            SetBackgroundColor(PLOT_CHROME_BG),
            Clear(ClearType::CurrentLine)
        )
        .context("clearing plot legend row")?;

        let mut used = 1usize;
        queue!(self.stdout, MoveTo(1, 1)).context("positioning plot legend row")?;
        for item in &frame.chrome.static_layer.legend {
            if used >= usize::from(size.width) {
                break;
            }
            used += print_status_text(
                &mut self.stdout,
                &item.marker,
                usize::from(size.width).saturating_sub(used),
                item.color,
            )
            .context("drawing plot legend marker")?;
            used += print_status_text(
                &mut self.stdout,
                &format!(" {}  ", item.label),
                usize::from(size.width).saturating_sub(used),
                PLOT_TEXT,
            )
            .context("drawing plot legend label")?;
        }
        queue!(self.stdout, ResetColor).context("resetting plot legend style")?;
        Ok(())
    }

    fn draw_plot_dynamic_chrome(
        &mut self,
        size: TerminalSize,
        frame: &PlotProtocolFrame<'_>,
    ) -> Result<()> {
        self.draw_plot_header(size, frame)?;
        Ok(())
    }

    fn draw_plot_axis_labels(&mut self, frame: &PlotProtocolFrame<'_>) -> Result<()> {
        for label in &frame.chrome.dynamic_layer.y_labels {
            queue!(
                self.stdout,
                MoveTo(0, label.row),
                SetBackgroundColor(PLOT_CHROME_BG),
                SetForegroundColor(PLOT_MUTED_TEXT),
                Print(trim_to_width(&label.text, usize::from(frame.body.col)))
            )
            .context("drawing plot y-axis label")?;
        }

        for label in &frame.chrome.dynamic_layer.x_labels {
            queue!(
                self.stdout,
                MoveTo(label.col, label.row),
                SetBackgroundColor(PLOT_CHROME_BG),
                SetForegroundColor(PLOT_MUTED_TEXT),
                Print(&label.text)
            )
            .context("drawing plot x-axis label")?;
        }
        queue!(self.stdout, ResetColor).context("resetting plot axis style")?;
        Ok(())
    }

    pub(crate) fn draw_plot_status_line(
        &mut self,
        size: TerminalSize,
        line: &ChromeLine,
    ) -> Result<()> {
        let status_row = size.height.saturating_sub(1);
        let content_width = size.width.max(1);
        draw_chrome_line(
            &mut self.stdout,
            status_row,
            content_width,
            Color::Rgb { r: 7, g: 10, b: 12 },
            Color::Rgb {
                r: 48,
                g: 64,
                b: 66,
            },
            line,
        )
        .context("drawing plot status line")?;
        Ok(())
    }

    fn draw_plot_readout_line(
        &mut self,
        size: TerminalSize,
        frame: &PlotProtocolFrame<'_>,
    ) -> Result<()> {
        if frame.chrome.dynamic_layer.readout_row >= size.height.saturating_sub(1) {
            return Ok(());
        }
        draw_chrome_line(
            &mut self.stdout,
            frame.chrome.dynamic_layer.readout_row,
            size.width.max(1),
            PLOT_CHROME_BG,
            Color::Rgb {
                r: 41,
                g: 54,
                b: 57,
            },
            &frame.chrome.dynamic_layer.readout,
        )
        .context("drawing plot readout line")?;
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

fn content_to_lines(content: &str, content_rows: u16, width: u16) -> String {
    let width = usize::from(width.max(1));
    let content_rows = usize::from(content_rows.max(1));
    let mut output = String::new();
    for (row, line) in content.lines().take(content_rows).enumerate() {
        if line.contains('\u{1b}') {
            output.push_str(line);
        } else {
            output.push_str(&trim_to_width(line, width));
        }
        if row + 1 < content_rows {
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
        let output = content_to_lines("alpha\nbeta", 2, 8);

        assert!(output.contains("alpha   \r\nbeta"));
        assert!(!output.contains("alpha   \nbeta"));
    }
}
