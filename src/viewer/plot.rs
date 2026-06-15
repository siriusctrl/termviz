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
    protocol: Protocol,
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
    let mut show_overlay = false;
    loop {
        let frame = if show_overlay {
            render_plot_overlay(&scene)
        } else {
            render_plot(&scene, request.kind, size)?
        };
        let status_text = status_line(&scene, request.kind, protocol, size, show_overlay);
        session.draw_frame(&frame, &status_text)?;

        match session.read_event()? {
            Some(Event::Resize(cols, rows)) => {
                size.width = cols.max(1);
                size.height = rows.max(1);
            }
            Some(Event::Key(key_event)) if key_event.kind == KeyEventKind::Press => {
                match key_event.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => break,
                    KeyCode::Char('m') | KeyCode::Char('M') => show_overlay = !show_overlay,
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
    protocol: Protocol,
    size: TerminalSize,
    show_overlay: bool,
) -> String {
    let overlay_hint = if show_overlay { "m back" } else { "m summary" };
    let status = format!(
        "{} | points={} | series={} | kind={:?} | {} | q quit",
        crate::render::protocols::protocol_name(protocol),
        scene.total_points(),
        scene.series.len(),
        kind,
        overlay_hint
    );
    let width = usize::from(size.width.max(1));
    trim_to_width(&status, width)
}

fn render_plot_overlay(scene: &PlotScene) -> String {
    let mut output = String::new();
    let points = scene.total_points();
    let series = scene.series.len();
    output.push_str(&format!("points: {points}\n"));
    output.push_str(&format!("series: {series}\n"));
    if let Some(bounds) = scene.bounds() {
        output.push_str(&format!("x: [{:.3}, {:.3}]\n", bounds.x_min, bounds.x_max));
        output.push_str(&format!("y: [{:.3}, {:.3}]\n", bounds.y_min, bounds.y_max));
    } else {
        output.push_str("bounds: unavailable\n");
    }
    output
}

pub(crate) struct PlotRequest {
    pub(crate) x: Option<String>,
    pub(crate) y: Option<String>,
    pub(crate) group: Option<String>,
    pub(crate) kind: PlotKind,
}

fn trim_to_width(text: &str, width: usize) -> String {
    if text.len() <= width {
        format!("{text:<width$}")
    } else {
        text.chars().take(width).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plot::model::{PlotPoint, PlotScene, PlotSeries};

    #[test]
    fn status_line_reports_overlay_hint() {
        let scene = PlotScene {
            title: None,
            series: Vec::new(),
        };
        let size = TerminalSize {
            width: 80,
            height: 24,
        };
        let normal = status_line(&scene, PlotKind::Line, Protocol::Blocks, size, false);
        let overlay = status_line(&scene, PlotKind::Line, Protocol::Blocks, size, true);
        assert!(normal.contains("m summary"));
        assert!(overlay.contains("m back"));
    }

    #[test]
    fn plot_overlay_contains_point_series_and_bounds() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "svc-a".to_owned(),
                points: vec![PlotPoint { x: 1.0, y: 20.0 }, PlotPoint { x: 2.0, y: 40.0 }],
            }],
        };

        let overlay = render_plot_overlay(&scene);
        assert!(overlay.contains("points: 2"));
        assert!(overlay.contains("series: 1"));
        assert!(overlay.contains("x: [1.000, 2.000]"));
        assert!(overlay.contains("y: [20.000, 40.000]"));
    }
}
