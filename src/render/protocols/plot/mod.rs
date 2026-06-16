use anyhow::Result;
use image::DynamicImage;

use crate::plot::{
    PlotKind,
    model::{PlotBounds, PlotScene},
};

mod layout;
mod raster;
mod text;
mod theme;

pub(crate) fn render_plot(scene: &PlotScene, kind: PlotKind) -> Result<DynamicImage> {
    raster::render_export_plot(scene, kind)
}

pub(crate) fn render_plot_for_bounds(
    scene: &PlotScene,
    kind: PlotKind,
    viewport: PlotBounds,
) -> Result<DynamicImage> {
    raster::render_export_plot_for_bounds(scene, kind, viewport)
}

pub(crate) fn render_interactive_plot_for_size(
    scene: &PlotScene,
    kind: PlotKind,
    viewport: PlotBounds,
    width: u32,
    height: u32,
) -> Result<DynamicImage> {
    raster::render_interactive_plot_for_size(scene, kind, viewport, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plot::model::{PlotPoint, PlotSeries};

    #[test]
    fn render_plot_includes_text_glyph_bytes() {
        let scene = sample_scene();

        let image = render_plot(&scene, PlotKind::Line).unwrap();
        let bytes = image.to_rgba8();
        assert!(
            bytes
                .pixels()
                .any(|pixel| pixel != &theme::EXPORT_BACKGROUND)
        );
    }

    #[test]
    fn render_plot_respects_viewport_bounds() {
        let scene = PlotScene {
            title: Some("zoom".to_owned()),
            series: vec![PlotSeries {
                name: "svc".to_owned(),
                points: vec![PlotPoint { x: 0.0, y: 0.0 }, PlotPoint { x: 10.0, y: 10.0 }],
            }],
        };
        let full = render_plot(&scene, PlotKind::Line).unwrap();
        let zoomed = render_plot_for_bounds(
            &scene,
            PlotKind::Line,
            PlotBounds {
                x_min: 4.0,
                x_max: 6.0,
                y_min: 4.0,
                y_max: 6.0,
            },
        )
        .unwrap();

        assert_ne!(full.to_rgba8().into_raw(), zoomed.to_rgba8().into_raw());
    }

    #[test]
    fn interactive_plot_uses_dark_background_without_changing_export_theme() {
        let scene = sample_scene();
        let bounds = scene.bounds().unwrap().normalized();

        let export = render_plot_for_bounds(&scene, PlotKind::Line, bounds)
            .unwrap()
            .to_rgba8();
        let interactive =
            render_interactive_plot_for_size(&scene, PlotKind::Line, bounds, 960, 496)
                .unwrap()
                .to_rgba8();

        assert_eq!(export.get_pixel(0, 0), &theme::EXPORT_BACKGROUND);
        assert_eq!(
            interactive.get_pixel(0, 0),
            &theme::INTERACTIVE_THEME.background
        );
        assert_ne!(export.get_pixel(0, 0), interactive.get_pixel(0, 0));
    }

    #[test]
    fn interactive_plot_renders_at_requested_target_size() {
        let scene = sample_scene();
        let bounds = scene.bounds().unwrap().normalized();

        let image =
            render_interactive_plot_for_size(&scene, PlotKind::Line, bounds, 960, 496).unwrap();

        assert_eq!(image.width(), 960);
        assert_eq!(image.height(), 496);
    }

    #[test]
    fn plot_layout_keeps_header_legend_outside_chart_area() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: (0..4)
                .map(|index| PlotSeries {
                    name: format!("service-{index}"),
                    points: vec![PlotPoint {
                        x: index as f64,
                        y: 100.0 + index as f64,
                    }],
                })
                .collect(),
        };

        let dimensions = layout::export_dimensions();
        let text = text::TextMetrics::new(1);
        let layout = layout::layout_for(dimensions, &scene, text);
        let legend_bottom = layout.legend_top
            + (scene.series.len().min(layout.legend_max_rows) as i32 - 1)
                * layout.legend_row_height
            + layout.text.glyph_height();

        assert!(legend_bottom < layout.area.top);
        assert!(layout.area.left > layout.text.width("10000.0"));
        assert!(
            i32::try_from(layout.dimensions.height).unwrap() - layout.area.bottom
                > layout.text.glyph_height() * 3
        );
    }

    #[test]
    fn render_plot_line_segment_crosses_viewport_when_endpoints_are_outside() {
        let scene = PlotScene {
            title: None,
            series: vec![PlotSeries {
                name: "crossing".to_owned(),
                points: vec![
                    PlotPoint { x: -5.0, y: -5.0 },
                    PlotPoint { x: 15.0, y: 15.0 },
                ],
            }],
        };

        let image = render_plot_for_bounds(
            &scene,
            PlotKind::Line,
            PlotBounds {
                x_min: 0.0,
                x_max: 10.0,
                y_min: 0.0,
                y_max: 10.0,
            },
        )
        .unwrap();
        let pixels = image.to_rgba8();
        let dimensions = layout::export_dimensions();
        let text = text::TextMetrics::new(1);
        let layout = layout::layout_for(dimensions, &scene, text);
        let expected_color = image::Rgba(theme::EXPORT_THEME.strokes[0]);

        let mut has_line_pixel = false;
        for y in layout.area.top..=layout.area.bottom {
            for x in layout.area.left..=layout.area.right {
                if pixels.get_pixel(x as u32, y as u32) == &expected_color {
                    has_line_pixel = true;
                    break;
                }
            }
            if has_line_pixel {
                break;
            }
        }

        assert!(
            has_line_pixel,
            "crossing segment should render a visible chart pixel"
        );
    }

    fn sample_scene() -> PlotScene {
        PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "svc".to_owned(),
                points: vec![PlotPoint { x: 1.0, y: 1.0 }, PlotPoint { x: 2.0, y: 1.5 }],
            }],
        }
    }
}
