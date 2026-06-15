use anyhow::{Context, Result, bail};
use crossterm::event::{Event, KeyCode, KeyEventKind};

use crate::{
    input::InputSource,
    plot::model::PlotScene,
    plot::{PlotKind, stream, table},
    profile::{ContentKind, InputProfile},
    render::Protocol,
    render::protocols::blocks,
    tui::{TerminalSession, TerminalSize},
};

pub(crate) fn run(
    source: InputSource,
    profile: InputProfile,
    _protocol: Protocol,
    request: PlotRequest,
) -> Result<()> {
    let scene = load_scene(
        &source,
        &profile,
        request.x.as_deref(),
        request.y.as_deref(),
        request.group.as_deref(),
    )?;
    let mut session = TerminalSession::start()?;
    let mut size = session.size().context("reading initial terminal size")?;
    let status =
        |size: TerminalSize| status_line(&scene, request.kind, request.protocol_note, size);
    loop {
        let frame = render_plot(&scene, request.kind, size)?;
        let status_text = status(size);
        session.draw_frame(&frame, &status_text)?;

        match session.read_event()? {
            Some(Event::Resize(cols, rows)) => {
                size.width = cols.max(1);
                size.height = rows.max(1);
            }
            Some(Event::Key(key_event)) if key_event.kind == KeyEventKind::Press => {
                match key_event.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => break,
                    _ => {}
                }
            }
            Some(_) | None => {}
        }
    }

    Ok(())
}

fn load_scene(
    source: &InputSource,
    profile: &InputProfile,
    x: Option<&str>,
    y: Option<&str>,
    group: Option<&str>,
) -> Result<PlotScene> {
    match profile.content {
        ContentKind::Csv | ContentKind::Tsv => {
            table::load_scene(source, x, y, group).context("loading plot scene from table data")
        }
        ContentKind::Jsonl => {
            stream::load_scene(source, x, y, group).context("loading plot scene from jsonl data")
        }
        _ => bail!("plot viewer only supports csv, tsv, and jsonl inputs"),
    }
}

fn render_plot(scene: &PlotScene, kind: PlotKind, size: TerminalSize) -> Result<String> {
    let cols = u32::from(size.width);
    let rows = u32::from(size.height.saturating_sub(1));
    let result = blocks::render_plot_for_size(scene, kind, cols.max(1), rows.max(1))
        .context("rendering plot scene for terminal")?;
    if result.is_empty() {
        Ok(String::new())
    } else {
        Ok(result)
    }
}

fn status_line(
    scene: &PlotScene,
    kind: PlotKind,
    protocol_note: Option<&str>,
    size: TerminalSize,
) -> String {
    let status = format!(
        "{} | points={} | series={} | kind={:?} | q quit",
        protocol_note.unwrap_or("protocol: blocks"),
        scene.total_points(),
        scene.series.len(),
        kind
    );
    let width = usize::from(size.width.max(1));
    trim_to_width(&status, width)
}

pub(crate) struct PlotRequest {
    pub(crate) x: Option<String>,
    pub(crate) y: Option<String>,
    pub(crate) group: Option<String>,
    pub(crate) kind: PlotKind,
    pub(crate) protocol_note: Option<&'static str>,
}

fn trim_to_width(text: &str, width: usize) -> String {
    if text.len() <= width {
        format!("{text:<width$}")
    } else {
        text.chars().take(width).collect()
    }
}
