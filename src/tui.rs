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

pub(crate) mod layout;
pub(crate) mod palette;

const EVENT_POLL_TIMEOUT: Duration = Duration::from_millis(8);
const PLOT_CHROME_BG: Color = Color::Rgb {
    r: 13,
    g: 17,
    b: 23,
};
const PLOT_TEXT: Color = Color::Rgb {
    r: 203,
    g: 213,
    b: 225,
};
const PLOT_MUTED_TEXT: Color = Color::Rgb {
    r: 148,
    g: 163,
    b: 184,
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

#[derive(Debug, Clone)]
pub(crate) struct PlotProtocolFrame<'a> {
    pub(crate) payload: &'a str,
    pub(crate) image_col: u16,
    pub(crate) image_row: u16,
    pub(crate) image_cols: u16,
    pub(crate) image_rows: u16,
    pub(crate) x_axis_row: u16,
    pub(crate) header: String,
    pub(crate) legend: Vec<PlotLegendItem>,
    pub(crate) y_labels: Vec<PlotAxisLabel>,
    pub(crate) x_labels: Vec<PlotAxisLabel>,
    pub(crate) status: &'a str,
}

#[derive(Debug, Clone)]
pub(crate) struct PlotLegendItem {
    pub(crate) marker: String,
    pub(crate) label: String,
    pub(crate) color: Color,
}

#[derive(Debug, Clone)]
pub(crate) struct PlotAxisLabel {
    pub(crate) col: u16,
    pub(crate) row: u16,
    pub(crate) text: String,
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
        if size.height > 0 {
            self.draw_status_line(size, status)?;
        }
        self.stdout.flush().context("flushing terminal frame")?;
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
        self.draw_plot_protocol_background(size, &frame)
            .context("drawing plot protocol chrome background")?;
        queue!(
            self.stdout,
            MoveTo(frame.image_col, frame.image_row),
            Print(frame.payload)
        )
        .context("drawing plot protocol payload")?;
        self.draw_plot_header(size, &frame)?;
        self.draw_plot_axis_labels(&frame)?;
        if size.height > 0 {
            self.draw_status_line(size, frame.status)?;
        }
        self.stdout
            .flush()
            .context("flushing plot protocol frame")?;
        Ok(())
    }

    fn draw_plot_protocol_background(
        &mut self,
        size: TerminalSize,
        frame: &PlotProtocolFrame<'_>,
    ) -> Result<()> {
        let header_rows = frame.image_row.min(size.height);
        for row in 0..header_rows {
            paint_row(&mut self.stdout, 0, row, size.width, PLOT_CHROME_BG)
                .context("painting plot header background")?;
        }

        let image_bottom = frame
            .image_row
            .saturating_add(frame.image_rows)
            .min(size.height);
        let gutter_rows = image_bottom.saturating_sub(frame.image_row);
        paint_rect(
            &mut self.stdout,
            0,
            frame.image_row,
            frame.image_col,
            gutter_rows,
            PLOT_CHROME_BG,
        )
        .context("painting plot y-axis gutter")?;

        if frame.x_axis_row < size.height {
            paint_row(
                &mut self.stdout,
                0,
                frame.x_axis_row,
                size.width,
                PLOT_CHROME_BG,
            )
            .context("painting plot x-axis row")?;
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
        draw_header_line(&mut self.stdout, 0, size.width, &frame.header)
            .context("drawing plot header")?;

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
        for item in &frame.legend {
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

    fn draw_plot_axis_labels(&mut self, frame: &PlotProtocolFrame<'_>) -> Result<()> {
        for label in &frame.y_labels {
            queue!(
                self.stdout,
                MoveTo(0, label.row),
                SetBackgroundColor(PLOT_CHROME_BG),
                SetForegroundColor(PLOT_MUTED_TEXT),
                Print(trim_to_width(&label.text, usize::from(frame.image_col)))
            )
            .context("drawing plot y-axis label")?;
        }

        for label in &frame.x_labels {
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

fn draw_status_chrome(
    stdout: &mut Stdout,
    row: u16,
    width: u16,
    text: &str,
) -> Result<(), io::Error> {
    let width = usize::from(width.max(1));
    let segments = status_segments(text);
    let mut used = 0usize;

    queue!(
        stdout,
        MoveTo(0, row),
        SetBackgroundColor(Color::Rgb { r: 9, g: 12, b: 18 }),
        SetForegroundColor(Color::Rgb {
            r: 148,
            g: 163,
            b: 184,
        }),
        Clear(ClearType::CurrentLine)
    )?;

    for (index, segment) in segments.iter().enumerate() {
        if used >= width {
            break;
        }

        if index > 0 {
            used += print_status_text(
                stdout,
                "  ",
                width.saturating_sub(used),
                Color::Rgb {
                    r: 71,
                    g: 85,
                    b: 105,
                },
            )?;
        }

        let color = status_segment_color(index, segment);
        used += print_status_text(stdout, segment, width.saturating_sub(used), color)?;
    }

    if used < width {
        queue!(stdout, Print(" ".repeat(width - used)))?;
    }
    queue!(stdout, ResetColor)?;
    Ok(())
}

fn draw_header_line(
    stdout: &mut Stdout,
    row: u16,
    width: u16,
    text: &str,
) -> Result<(), io::Error> {
    let width = usize::from(width.max(1));
    let segments = status_segments(text);
    let mut used = 0usize;
    queue!(
        stdout,
        MoveTo(0, row),
        SetBackgroundColor(PLOT_CHROME_BG),
        Clear(ClearType::CurrentLine)
    )?;

    for (index, segment) in segments.iter().enumerate() {
        if used >= width {
            break;
        }
        if index > 0 {
            used += print_status_text(
                stdout,
                "  ",
                width.saturating_sub(used),
                Color::Rgb {
                    r: 51,
                    g: 65,
                    b: 85,
                },
            )?;
        }
        let color = if index == 0 {
            Color::Rgb {
                r: 226,
                g: 232,
                b: 240,
            }
        } else {
            Color::Rgb {
                r: 148,
                g: 163,
                b: 184,
            }
        };
        used += print_status_text(stdout, segment, width.saturating_sub(used), color)?;
    }

    if used < width {
        queue!(stdout, Print(" ".repeat(width - used)))?;
    }
    queue!(stdout, ResetColor)?;
    Ok(())
}

fn paint_rect(
    stdout: &mut Stdout,
    col: u16,
    row: u16,
    width: u16,
    height: u16,
    background: Color,
) -> Result<(), io::Error> {
    if width == 0 || height == 0 {
        return Ok(());
    }
    for offset in 0..height {
        paint_row(stdout, col, row.saturating_add(offset), width, background)?;
    }
    Ok(())
}

fn paint_row(
    stdout: &mut Stdout,
    col: u16,
    row: u16,
    width: u16,
    background: Color,
) -> Result<(), io::Error> {
    if width == 0 {
        return Ok(());
    }
    queue!(
        stdout,
        MoveTo(col, row),
        SetBackgroundColor(background),
        Print(" ".repeat(usize::from(width)))
    )?;
    Ok(())
}

fn status_segments(text: &str) -> Vec<&str> {
    text.split(" · ")
        .flat_map(|segment| segment.split(" | "))
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn status_segment_color(index: usize, segment: &str) -> Color {
    if index == 0 {
        return Color::Rgb {
            r: 125,
            g: 211,
            b: 252,
        };
    }
    if segment == "fit" || segment == "pan/zoom" || segment.ends_with(" info") {
        return Color::Rgb {
            r: 226,
            g: 232,
            b: 240,
        };
    }
    if segment.contains("quit") || segment.contains("zoom") || segment.contains("pan") {
        return Color::Rgb {
            r: 251,
            g: 191,
            b: 36,
        };
    }
    Color::Rgb {
        r: 148,
        g: 163,
        b: 184,
    }
}

fn print_status_text(
    stdout: &mut Stdout,
    text: &str,
    width: usize,
    color: Color,
) -> Result<usize, io::Error> {
    let clipped = clip_to_width(text, width);
    let used = display_width(&clipped);
    queue!(stdout, SetForegroundColor(color), Print(clipped))?;
    Ok(used)
}

fn clip_to_width(text: &str, width: usize) -> String {
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
    output
}

fn display_width(text: &str) -> usize {
    text.chars()
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(1).max(1))
        .sum()
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

    #[test]
    fn status_segments_accept_new_and_legacy_separators() {
        assert_eq!(
            status_segments("kitty · fit · q quit"),
            vec!["kitty", "fit", "q quit"]
        );
        assert_eq!(
            status_segments("protocol: blocks | fit | q quit"),
            vec!["protocol: blocks", "fit", "q quit"]
        );
    }
}
