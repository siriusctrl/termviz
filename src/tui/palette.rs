use ratatui::style::{Color, Style};

pub(crate) fn plain_style() -> Style {
    Style::default().fg(Color::Reset)
}
