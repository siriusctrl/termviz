pub(crate) mod histogram;
pub(crate) mod model;
pub(crate) mod stream;
pub(crate) mod table;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlotKind {
    Line,
    Scatter,
    Bar,
    Area,
    Histogram,
}

impl PlotKind {
    pub(crate) fn requires_y(self) -> bool {
        !matches!(self, Self::Histogram)
    }

    pub(crate) fn missing_fields_reason(self) -> &'static str {
        if self.requires_y() {
            "x and y required for plot scene loading"
        } else {
            "x required for histogram plot scene loading"
        }
    }

    pub(crate) fn render_bounds(self, scene: &model::PlotScene) -> Option<model::PlotBounds> {
        let mut bounds = scene.bounds()?;
        if matches!(self, Self::Bar | Self::Area | Self::Histogram) {
            bounds.y_min = bounds.y_min.min(0.0);
            bounds.y_max = bounds.y_max.max(0.0);
        }
        if matches!(self, Self::Bar | Self::Histogram) {
            let pad = x_step(scene).unwrap_or(1.0) * 0.5;
            bounds.x_min -= pad;
            bounds.x_max += pad;
        }
        Some(bounds.normalized())
    }
}

fn x_step(scene: &model::PlotScene) -> Option<f64> {
    let mut xs = scene
        .series
        .iter()
        .flat_map(|series| series.points.iter())
        .map(|point| point.x)
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    xs.sort_by(|left, right| left.total_cmp(right));
    xs.dedup_by(|left, right| (*left - *right).abs() <= f64::EPSILON);

    xs.windows(2)
        .filter_map(|pair| {
            let distance = pair[1] - pair[0];
            (distance > f64::EPSILON).then_some(distance)
        })
        .min_by(|left, right| left.total_cmp(right))
}
