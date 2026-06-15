#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlotScene {
    pub(crate) title: Option<String>,
    pub(crate) series: Vec<PlotSeries>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlotSeries {
    pub(crate) name: String,
    pub(crate) points: Vec<PlotPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlotPoint {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlotBounds {
    pub(crate) x_min: f64,
    pub(crate) x_max: f64,
    pub(crate) y_min: f64,
    pub(crate) y_max: f64,
}

impl PlotBounds {
    pub(crate) fn normalized(self) -> Self {
        let mut bounds = self;
        const MIN_SPAN: f64 = 1e-6;

        let x_span = (bounds.x_max - bounds.x_min).abs();
        if !x_span.is_finite() || x_span < MIN_SPAN {
            let mid = (bounds.x_min + bounds.x_max) / 2.0;
            bounds.x_min = mid - 0.5;
            bounds.x_max = mid + 0.5;
        }

        let y_span = (bounds.y_max - bounds.y_min).abs();
        if !y_span.is_finite() || y_span < MIN_SPAN {
            let mid = (bounds.y_min + bounds.y_max) / 2.0;
            bounds.y_min = mid - 0.5;
            bounds.y_max = mid + 0.5;
        }

        bounds
    }
}

impl PlotScene {
    pub(crate) fn total_points(&self) -> usize {
        self.series.iter().map(|series| series.points.len()).sum()
    }

    pub(crate) fn bounds(&self) -> Option<PlotBounds> {
        let mut first = true;
        let mut x_min = 0.0;
        let mut x_max = 0.0;
        let mut y_min = 0.0;
        let mut y_max = 0.0;

        for point in self.series.iter().flat_map(|series| series.points.iter()) {
            if first {
                x_min = point.x;
                x_max = point.x;
                y_min = point.y;
                y_max = point.y;
                first = false;
            } else {
                x_min = x_min.min(point.x);
                x_max = x_max.max(point.x);
                y_min = y_min.min(point.y);
                y_max = y_max.max(point.y);
            }
        }

        if first {
            return None;
        }

        Some(PlotBounds {
            x_min,
            x_max,
            y_min,
            y_max,
        })
    }

    pub(crate) fn to_svg(&self, kind: crate::plot::PlotKind) -> String {
        if self.series.is_empty() {
            return "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"640\" height=\"360\"></svg>\n"
                .to_owned();
        }

        let width = 640.0f64;
        let height = 360.0f64;
        let left = 52.0;
        let right = 16.0;
        let top = 16.0;
        let bottom = 24.0;
        let plot_width = (width - left - right).max(1.0);
        let plot_height = (height - top - bottom).max(1.0);
        let x_max = left + plot_width;
        let y_max = top + plot_height;
        let bounds = self.bounds().unwrap_or(PlotBounds {
            x_min: 0.0,
            x_max: 1.0,
            y_min: 0.0,
            y_max: 1.0,
        });

        let mut output = String::from(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 640 360\" width=\"640\" height=\"360\">",
        );
        output.push_str(&format!(
            "<rect x=\"{left:.0}\" y=\"{top:.0}\" width=\"{plot_width:.0}\" height=\"{plot_height:.0}\" fill=\"none\" stroke=\"#000\" />"
        ));
        output.push_str(&format!(
            "<line x1=\"{left:.0}\" y1=\"{y_max:.0}\" x2=\"{x_max:.0}\" y2=\"{y_max:.0}\" stroke=\"#444\" />"
        ));
        output.push_str(&format!(
            "<line x1=\"{left:.0}\" y1=\"{top:.0}\" x2=\"{left:.0}\" y2=\"{y_max:.0}\" stroke=\"#444\" />"
        ));

        let colors = [
            "#1f77b4", "#ff7f0e", "#2ca02c", "#d62728", "#9467bd", "#8c564b",
        ];

        for (series_index, series) in self.series.iter().enumerate() {
            if series.points.is_empty() {
                continue;
            }

            let points = series
                .points
                .iter()
                .map(|point| {
                    let x = map_coord(point.x, bounds.x_min, bounds.x_max, left, plot_width);
                    let y = map_coord(
                        point.y,
                        bounds.y_min,
                        bounds.y_max,
                        top + plot_height,
                        -plot_height,
                    );
                    format!("{x:.2} {y:.2}")
                })
                .collect::<Vec<_>>();

            match kind {
                crate::plot::PlotKind::Line => {
                    let mut commands = String::new();
                    for (index, point) in points.iter().enumerate() {
                        if index == 0 {
                            let _ = std::fmt::Write::write_fmt(
                                &mut commands,
                                format_args!("M {point}"),
                            );
                        } else {
                            let _ = std::fmt::Write::write_fmt(
                                &mut commands,
                                format_args!(" L {point}"),
                            );
                        }
                    }
                    let color = colors[series_index % colors.len()];
                    output.push_str(&format!(
                        "<path d=\"{commands}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"2\" />"
                    ));
                }
                crate::plot::PlotKind::Scatter => {
                    let color = colors[series_index % colors.len()];
                    for point in points {
                        let mut parts = point.split_whitespace();
                        let px = parts.next().unwrap_or("0");
                        let py = parts.next().unwrap_or("0");
                        output.push_str(&format!(
                            "<circle cx=\"{px}\" cy=\"{py}\" r=\"2.2\" fill=\"{color}\" />"
                        ));
                    }
                }
            }
        }

        output.push_str(&format!(
            "<text x=\"20\" y=\"18\" font-size=\"12\" font-family=\"monospace\">points: {}</text>",
            self.total_points()
        ));
        output.push_str("</svg>");
        output
    }
}

fn map_coord(value: f64, lower: f64, upper: f64, origin: f64, span: f64) -> f64 {
    let span_values = (upper - lower).abs().max(f64::EPSILON);
    let ratio = (value - lower) / span_values;
    let clamped = ratio.clamp(0.0, 1.0);
    origin + span * clamped
}
