use std::io::{self, Stdout};

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};
use unicode_width::UnicodeWidthChar;

use super::{ChromeLine, ChromeRole, PLOT_MUTED_TEXT};

pub(super) fn draw_status_chrome(
    stdout: &mut Stdout,
    row: u16,
    width: u16,
    text: &str,
) -> Result<(), io::Error> {
    let width = usize::from(width.max(1));
    let line = ChromeLine::new(
        status_segments(text)
            .iter()
            .enumerate()
            .map(|(index, segment)| {
                super::ChromeSegment::new(*segment, status_segment_role(index, segment))
            })
            .collect::<Vec<_>>(),
    );
    draw_chrome_line(
        stdout,
        row,
        width as u16,
        Color::Rgb { r: 7, g: 10, b: 12 },
        Color::Rgb {
            r: 48,
            g: 64,
            b: 66,
        },
        &line,
    )
}

pub(super) fn draw_chrome_line(
    stdout: &mut Stdout,
    row: u16,
    width: u16,
    background: Color,
    separator: Color,
    line: &ChromeLine,
) -> Result<(), io::Error> {
    let width = usize::from(width.max(1));
    let mut used = 0usize;

    queue!(
        stdout,
        MoveTo(0, row),
        SetBackgroundColor(background),
        SetForegroundColor(chrome_role_color(ChromeRole::Muted)),
        Clear(ClearType::CurrentLine)
    )?;

    for (index, segment) in line.segments.iter().enumerate() {
        if used >= width {
            break;
        }

        if index > 0 {
            used += print_status_text(stdout, "  ", width.saturating_sub(used), separator)?;
        }

        used += print_status_text(
            stdout,
            &segment.text,
            width.saturating_sub(used),
            chrome_role_color(segment.role),
        )?;
    }

    if used < width {
        queue!(stdout, Print(" ".repeat(width - used)))?;
    }
    queue!(stdout, ResetColor)?;
    Ok(())
}

pub(super) fn paint_rect(
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

pub(super) fn paint_row(
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

pub(super) fn print_status_text(
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

fn status_segments(text: &str) -> Vec<&str> {
    text.split(" · ")
        .flat_map(|segment| segment.split(" | "))
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn status_segment_role(index: usize, segment: &str) -> ChromeRole {
    if index == 0 {
        return ChromeRole::State;
    }
    if segment == "fit" || segment == "pan/zoom" || segment.ends_with(" info") {
        return ChromeRole::State;
    }
    if segment.contains("quit") || segment.contains("zoom") || segment.contains("pan") {
        return ChromeRole::Action;
    }
    ChromeRole::Muted
}

fn chrome_role_color(role: ChromeRole) -> Color {
    match role {
        ChromeRole::Title => Color::Rgb {
            r: 238,
            g: 243,
            b: 234,
        },
        ChromeRole::Meta => Color::Rgb {
            r: 145,
            g: 160,
            b: 158,
        },
        ChromeRole::State => Color::Rgb {
            r: 86,
            g: 199,
            b: 217,
        },
        ChromeRole::Action => Color::Rgb {
            r: 242,
            g: 166,
            b: 90,
        },
        ChromeRole::Muted => PLOT_MUTED_TEXT,
    }
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
