use std::io::{BufRead, BufReader};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::{
    input::InputSource,
    plot::histogram::{self, HistogramValues},
    plot::model::{PlotPoint, PlotScene, PlotSeries},
};

const MAX_STREAM_RECORDS: usize = 1_024;
const DEFAULT_STREAM_SERIES_NAME: &str = "all";

pub(crate) fn load_scene(
    source: &InputSource,
    x: Option<&str>,
    y: Option<&str>,
    group: Option<&str>,
) -> Result<PlotScene> {
    let x_name = require_field(x, "--x")?;
    let y_name = require_field(y, "--y")?;

    let file = source
        .open()
        .with_context(|| format!("opening {}", source.label()))?;
    let mut scene = PlotScene {
        title: Some(source.label().to_owned()),
        series: Vec::new(),
    };

    let mut grouped = std::collections::HashMap::<String, usize>::new();
    let mut record_count = 0usize;

    for (line_number, line) in BufReader::new(file).lines().enumerate() {
        if record_count >= MAX_STREAM_RECORDS {
            break;
        }

        let line = line
            .with_context(|| format!("reading {} at line {}", source.label(), line_number + 1))?;
        if line.trim().is_empty() {
            continue;
        }

        let value: Value = serde_json::from_str(&line)
            .with_context(|| format!("parsing json at line {}", line_number + 1))?;

        let record = value.as_object().ok_or_else(|| {
            anyhow::anyhow!(
                "line {} is not a JSON object: {}",
                line_number + 1,
                source.label()
            )
        })?;

        let x = parse_numeric(record, x_name, "--x", line_number + 1, source.label())?;
        let y = parse_numeric(record, y_name, "--y", line_number + 1, source.label())?;

        let series_name = group
            .map(|name| {
                record.get(name).map(value_to_series_label).ok_or_else(|| {
                    anyhow::anyhow!(
                        "line {} missing required --group field '{name}' in {}",
                        line_number + 1,
                        source.label()
                    )
                })
            })
            .transpose()?
            .unwrap_or_else(|| DEFAULT_STREAM_SERIES_NAME.to_owned());
        let series_name = if series_name.is_empty() {
            DEFAULT_STREAM_SERIES_NAME.to_owned()
        } else {
            series_name
        };

        let series_index = match grouped.get(&series_name).copied() {
            Some(index) => index,
            None => {
                scene.series.push(PlotSeries {
                    name: series_name.clone(),
                    points: Vec::new(),
                });
                let index = scene.series.len() - 1;
                grouped.insert(series_name, index);
                index
            }
        };
        scene.series[series_index].points.push(PlotPoint { x, y });
        record_count += 1;
    }

    if record_count == 0 {
        bail!(
            "no data rows found in {}; jsonl input must contain object records",
            source.label()
        );
    }

    Ok(scene)
}

pub(crate) fn load_histogram_scene(
    source: &InputSource,
    value: Option<&str>,
    group: Option<&str>,
) -> Result<PlotScene> {
    let value_name = require_field(value, "--x")?;

    let file = source
        .open()
        .with_context(|| format!("opening {}", source.label()))?;
    let mut series_values = Vec::<HistogramValues>::new();
    let mut grouped = std::collections::HashMap::<String, usize>::new();
    let mut record_count = 0usize;

    for (line_number, line) in BufReader::new(file).lines().enumerate() {
        if record_count >= MAX_STREAM_RECORDS {
            break;
        }

        let line = line
            .with_context(|| format!("reading {} at line {}", source.label(), line_number + 1))?;
        if line.trim().is_empty() {
            continue;
        }

        let value: Value = serde_json::from_str(&line)
            .with_context(|| format!("parsing json at line {}", line_number + 1))?;

        let record = value.as_object().ok_or_else(|| {
            anyhow::anyhow!(
                "line {} is not a JSON object: {}",
                line_number + 1,
                source.label()
            )
        })?;

        let value = parse_numeric(record, value_name, "--x", line_number + 1, source.label())?;
        let series_name = group
            .map(|name| {
                record.get(name).map(value_to_series_label).ok_or_else(|| {
                    anyhow::anyhow!(
                        "line {} missing required --group field '{name}' in {}",
                        line_number + 1,
                        source.label()
                    )
                })
            })
            .transpose()?
            .unwrap_or_else(|| DEFAULT_STREAM_SERIES_NAME.to_owned());
        let series_name = if series_name.is_empty() {
            DEFAULT_STREAM_SERIES_NAME.to_owned()
        } else {
            series_name
        };

        let series_index = match grouped.get(&series_name).copied() {
            Some(index) => index,
            None => {
                series_values.push(HistogramValues {
                    name: series_name.clone(),
                    values: Vec::new(),
                });
                let index = series_values.len() - 1;
                grouped.insert(series_name, index);
                index
            }
        };
        series_values[series_index].values.push(value);
        record_count += 1;
    }

    if record_count == 0 {
        bail!(
            "no data rows found in {}; jsonl input must contain object records",
            source.label()
        );
    }

    histogram::build_scene(Some(source.label().to_owned()), series_values)
}

fn parse_numeric(
    record: &serde_json::Map<String, Value>,
    field: &str,
    flag: &str,
    line_number: usize,
    label: &str,
) -> Result<f64> {
    let Some(value) = record.get(field) else {
        bail!("line {line_number} missing required {flag} field '{field}' in {label}")
    };

    match value {
        Value::Number(number) => number.as_f64().ok_or_else(|| {
            anyhow::anyhow!(
                "line {line_number} {flag} field '{field}' in {label} is not a finite number"
            )
        }),
        Value::String(value) => value.parse::<f64>().map_err(|_| {
            anyhow::anyhow!(
                "line {line_number} {flag} field '{field}' in {label} is not numeric: '{value}'"
            )
        }),
        _ => Err(anyhow::anyhow!(
            "line {line_number} {flag} field '{field}' in {label} is not numeric"
        )),
    }
}

fn value_to_series_label(value: &Value) -> String {
    match value {
        Value::String(text) => text.to_owned(),
        _ => value.to_string(),
    }
}

fn require_field<'a>(value: Option<&'a str>, flag: &str) -> Result<&'a str> {
    value.ok_or_else(|| {
        anyhow::anyhow!("plot loading requires {flag}; provide the field name using {flag} <name>")
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn loads_simple_jsonl_records() {
        let mut file = NamedTempFile::with_suffix(".jsonl").unwrap();
        writeln!(file, "{{\"ts\":1,\"value\":20,\"service\":\"api\"}}").unwrap();
        writeln!(file, "{{\"ts\":2,\"value\":30,\"service\":\"api\"}}").unwrap();
        writeln!(file, "{{\"ts\":3,\"value\":40,\"service\":\"worker\"}}").unwrap();

        let source = InputSource::from_path(file.path().to_path_buf()).unwrap();
        let scene = load_scene(&source, Some("ts"), Some("value"), Some("service")).unwrap();

        assert_eq!(scene.series.len(), 2);
        assert_eq!(scene.series[0].name, "api");
        assert_eq!(scene.series[1].name, "worker");
        assert_eq!(scene.series[1].points[0], PlotPoint { x: 3.0, y: 40.0 });
    }

    #[test]
    fn jsonl_requires_required_fields() {
        let mut file = NamedTempFile::with_suffix(".jsonl").unwrap();
        writeln!(file, "{{\"ts\":1}}").unwrap();

        let source = InputSource::from_path(file.path().to_path_buf()).unwrap();
        let err = load_scene(&source, Some("ts"), Some("value"), None).unwrap_err();
        assert!(err.to_string().contains("missing required --y"));
    }

    #[test]
    fn stream_scene_is_bounded() {
        let mut file = NamedTempFile::with_suffix(".jsonl").unwrap();
        for i in 0..(MAX_STREAM_RECORDS + 100) {
            writeln!(file, "{{\"ts\":{i},\"value\":{i}}}").unwrap();
        }

        let source = InputSource::from_path(file.path().to_path_buf()).unwrap();
        let scene = load_scene(&source, Some("ts"), Some("value"), None).unwrap();

        let loaded = scene
            .series
            .iter()
            .map(|series| series.points.len())
            .sum::<usize>();
        assert_eq!(loaded, MAX_STREAM_RECORDS);
    }

    #[test]
    fn loads_histogram_from_jsonl_values() {
        let mut file = NamedTempFile::with_suffix(".jsonl").unwrap();
        writeln!(file, "{{\"latency\":10,\"service\":\"api\"}}").unwrap();
        writeln!(file, "{{\"latency\":12,\"service\":\"api\"}}").unwrap();
        writeln!(file, "{{\"latency\":20,\"service\":\"worker\"}}").unwrap();

        let source = InputSource::from_path(file.path().to_path_buf()).unwrap();
        let scene = load_histogram_scene(&source, Some("latency"), Some("service")).unwrap();

        assert_eq!(scene.series.len(), 2);
        assert_eq!(
            scene
                .series
                .iter()
                .flat_map(|series| series.points.iter())
                .map(|point| point.y)
                .sum::<f64>(),
            3.0
        );
    }
}
