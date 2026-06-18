use anyhow::Result;
use image::Rgba;

use crate::plot::{
    PlotKind,
    model::{PlotBounds, PlotScene},
};

use super::{
    display_list::{LineStyle, PlotCommand, PlotDisplayList, build_display_list},
    layout::export_dimensions,
    text::TextMetrics,
    theme::EXPORT_THEME,
};

pub(super) fn render_svg(scene: &PlotScene, kind: PlotKind) -> Result<String> {
    let viewport = kind.render_bounds(scene).unwrap_or(PlotBounds {
        x_min: 0.0,
        x_max: 1.0,
        y_min: 0.0,
        y_max: 1.0,
    });
    let dimensions = export_dimensions();
    let list = build_display_list(
        scene,
        kind,
        viewport,
        dimensions,
        EXPORT_THEME,
        TextMetrics::new(1),
    );

    Ok(render_display_list(&list))
}

fn render_display_list(list: &PlotDisplayList) -> String {
    let mut output = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\" shape-rendering=\"geometricPrecision\">",
        list.dimensions.width,
        list.dimensions.height,
        list.dimensions.width,
        list.dimensions.height
    );
    output.push_str(&format!(
        "<rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"{}\" />",
        list.dimensions.width,
        list.dimensions.height,
        rgba_to_hex(list.background),
    ));

    for command in &list.commands {
        match command {
            PlotCommand::Line {
                start,
                end,
                color,
                style,
                width,
            } => {
                let dash = match style {
                    LineStyle::Solid => "",
                    LineStyle::Dotted => " stroke-dasharray=\"1 3\"",
                };
                output.push_str(&format!(
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\"{} />",
                    start.x,
                    start.y,
                    end.x,
                    end.y,
                    rgba_to_hex(*color),
                    (*width).max(1),
                    dash,
                ));
            }
            PlotCommand::Dot {
                center,
                radius,
                color,
            } => {
                output.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" />",
                    center.x,
                    center.y,
                    (*radius).max(1),
                    rgba_to_hex(*color),
                ));
            }
            PlotCommand::Rect {
                left,
                top,
                right,
                bottom,
                color,
            } => {
                let opacity = svg_opacity(*color);
                output.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{} />",
                    left.min(right),
                    top.min(bottom),
                    (right - left).abs() + 1,
                    (bottom - top).abs() + 1,
                    rgba_to_hex(*color),
                    opacity,
                ));
            }
            PlotCommand::Text {
                origin,
                content,
                color,
                max_width,
            } => {
                let content = match max_width {
                    Some(max_width) => clip_text(content, list.text, *max_width),
                    None => content.to_owned(),
                };
                output.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\" fill=\"{}\" font-size=\"{}\" font-family=\"monospace\" dominant-baseline=\"hanging\">{}</text>",
                    origin.x,
                    origin.y,
                    rgba_to_hex(*color),
                    list.text.glyph_height(),
                    escape_xml(&content),
                ));
            }
        }
    }

    output.push_str("</svg>\n");
    output
}

fn clip_text(content: &str, metrics: TextMetrics, max_width: i32) -> String {
    if max_width < metrics.glyph_width() {
        return String::new();
    }

    let mut clipped = String::new();
    for ch in content.chars() {
        let candidate_width = metrics.width(&format!("{clipped}{ch}"));
        if candidate_width > max_width {
            break;
        }
        clipped.push(ch);
    }
    clipped
}

fn rgba_to_hex(color: Rgba<u8>) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

fn svg_opacity(color: Rgba<u8>) -> String {
    if color[3] == u8::MAX {
        String::new()
    } else {
        format!(" fill-opacity=\"{:.3}\"", f64::from(color[3]) / 255.0)
    }
}

fn escape_xml(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            _ => output.push(ch),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plot::model::{PlotPoint, PlotSeries};

    #[test]
    fn render_svg_uses_shared_plot_summary_and_series_color() {
        let scene = PlotScene {
            title: Some("latency & load".to_owned()),
            series: vec![PlotSeries {
                name: "svc".to_owned(),
                points: vec![PlotPoint { x: 0.0, y: 1.0 }, PlotPoint { x: 1.0, y: 2.0 }],
            }],
        };

        let svg = render_svg(&scene, PlotKind::Line).unwrap();

        assert!(svg.starts_with("<svg "));
        assert!(svg.contains("latency &amp; load"));
        assert!(svg.contains("#1f77b4"));
        assert!(svg.contains("points: 2"));
    }

    #[test]
    fn render_svg_includes_rectangles_for_bar_plots() {
        let scene = PlotScene {
            title: Some("errors".to_owned()),
            series: vec![PlotSeries {
                name: "svc".to_owned(),
                points: vec![PlotPoint { x: 1.0, y: 2.0 }, PlotPoint { x: 2.0, y: 4.0 }],
            }],
        };

        let svg = render_svg(&scene, PlotKind::Bar).unwrap();

        assert!(svg.contains("<rect "));
        assert!(svg.contains("#1f77b4"));
    }
}
