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
}
