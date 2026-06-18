use anyhow::{Result, bail};

use crate::plot::model::{PlotPoint, PlotScene, PlotSeries};

pub(crate) const DEFAULT_HISTOGRAM_BINS: usize = 12;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HistogramValues {
    pub(crate) name: String,
    pub(crate) values: Vec<f64>,
}

pub(crate) fn build_scene(
    title: Option<String>,
    series_values: Vec<HistogramValues>,
) -> Result<PlotScene> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut total_values = 0usize;

    for series in &series_values {
        for value in &series.values {
            min = min.min(*value);
            max = max.max(*value);
            total_values += 1;
        }
    }

    if total_values == 0 {
        bail!("histogram input must contain at least one numeric value")
    }

    let bin_count = DEFAULT_HISTOGRAM_BINS.min(total_values.max(1));
    let span = (max - min).abs();
    let (start, bin_width) = if span <= f64::EPSILON {
        (min - 0.5, 1.0 / bin_count as f64)
    } else {
        (min, span / bin_count as f64)
    };

    let mut scene = PlotScene {
        title,
        series: Vec::with_capacity(series_values.len()),
    };

    for series in series_values {
        let mut counts = vec![0usize; bin_count];
        for value in series.values {
            let offset = ((value - start) / bin_width).floor();
            let index = if offset.is_finite() {
                (offset as isize).clamp(0, bin_count as isize - 1) as usize
            } else {
                0
            };
            counts[index] += 1;
        }

        let points = counts
            .into_iter()
            .enumerate()
            .map(|(index, count)| PlotPoint {
                x: start + (index as f64 + 0.5) * bin_width,
                y: count as f64,
            })
            .collect();

        scene.series.push(PlotSeries {
            name: series.name,
            points,
        });
    }

    Ok(scene)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_bounded_histogram_bins() {
        let scene = build_scene(
            Some("latency".to_owned()),
            vec![HistogramValues {
                name: "all".to_owned(),
                values: vec![1.0, 1.5, 2.0, 12.0],
            }],
        )
        .unwrap();

        assert_eq!(scene.series.len(), 1);
        assert_eq!(scene.total_points(), 4);
        assert_eq!(
            scene.series[0]
                .points
                .iter()
                .map(|point| point.y)
                .sum::<f64>(),
            4.0
        );
    }
}
