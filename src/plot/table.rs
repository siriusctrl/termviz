use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
};

use anyhow::{Context, Result, bail};

use crate::{
    input::InputSource,
    plot::model::{PlotPoint, PlotScene, PlotSeries},
};

const MAX_TABLE_ROWS: usize = 1_024;
const DEFAULT_SERIES_NAME: &str = "all";

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
    let mut lines = BufReader::new(file).lines();

    let mut header_line = None;
    for line in lines.by_ref() {
        let line = line.with_context(|| format!("reading header from {}", source.label()))?;
        if !line.trim().is_empty() {
            header_line = Some(line);
            break;
        }
    }
    let header_line = header_line.ok_or_else(|| {
        anyhow::anyhow!(
            "could not parse table header from {}; expected a header row for --x/--y lookup",
            source.label()
        )
    })?;
    let headers = parse_row(&header_line, delimiter_from_source(source)?);

    let x_index = headers
        .iter()
        .position(|header| header == x_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "missing --x column '{x_name}' in table header: {}",
                source.label()
            )
        })?;
    let y_index = headers
        .iter()
        .position(|header| header == y_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "missing --y column '{y_name}' in table header: {}",
                source.label()
            )
        })?;
    let group_index = match group {
        Some(name) => Some(
            headers
                .iter()
                .position(|header| header == name)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "missing --group column '{name}' in table header: {}",
                        source.label()
                    )
                })?,
        ),
        None => None,
    };

    let mut scene = PlotScene {
        title: Some(source.label().to_owned()),
        series: Vec::new(),
    };
    let mut series_by_name = HashMap::<String, usize>::new();

    let mut sample_rows = 0usize;
    for line in lines {
        if sample_rows >= MAX_TABLE_ROWS {
            break;
        }
        let line = line.with_context(|| format!("reading table rows from {}", source.label()))?;
        if line.trim().is_empty() {
            continue;
        }

        let row = parse_row(&line, delimiter_from_source(source)?);
        let expected_columns = x_index.max(y_index).max(group_index.unwrap_or(0));
        if row.len() <= expected_columns {
            bail!(
                "line {} has only {} columns, expected at least {}",
                sample_rows + 2,
                row.len(),
                expected_columns + 1
            );
        }

        let x = parse_number(&row[x_index], "--x", source.label())?;
        let y = parse_number(&row[y_index], "--y", source.label())?;
        let series_name = group_index
            .map(|index| row[index].clone())
            .unwrap_or_else(|| DEFAULT_SERIES_NAME.to_owned());
        let series_name = if series_name.is_empty() {
            DEFAULT_SERIES_NAME.to_owned()
        } else {
            series_name
        };

        let series_index = if let Some(index) = series_by_name.get(&series_name).copied() {
            index
        } else {
            scene.series.push(PlotSeries {
                name: series_name.clone(),
                points: Vec::new(),
            });
            let index = scene.series.len() - 1;
            series_by_name.insert(series_name, index);
            index
        };
        scene.series[series_index].points.push(PlotPoint { x, y });
        sample_rows += 1;
    }

    if sample_rows == 0 {
        bail!(
            "no data rows found in {}; table input must contain data after the header",
            source.label()
        );
    }

    Ok(scene)
}

fn delimiter_from_source(source: &InputSource) -> Result<u8> {
    let extension = source
        .path()
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_ascii_lowercase);

    let Some(ext) = extension else {
        bail!("could not determine table delimiter for {}", source.label())
    };

    match ext.as_str() {
        "tsv" => Ok(b'\t'),
        _ => Ok(b','),
    }
}

fn parse_row(line: &str, delimiter: u8) -> Vec<String> {
    line.split(char::from(delimiter))
        .map(|field| field.trim().trim_matches('\r').to_string())
        .collect()
}

fn parse_number(value: &str, field: &str, label: &str) -> Result<f64> {
    let trimmed = value.trim();
    trimmed
        .parse::<f64>()
        .map_err(|_| anyhow::anyhow!("invalid numeric value for {field} in {label}: '{value}'"))
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
    fn loads_csv_with_named_groups() {
        let mut file = NamedTempFile::with_suffix(".csv").unwrap();
        writeln!(file, "time,latency,service").unwrap();
        writeln!(file, "1,20,api").unwrap();
        writeln!(file, "2,30,api").unwrap();
        writeln!(file, "3,40,worker").unwrap();

        let source = InputSource::from_path(file.path().to_path_buf()).unwrap();
        let scene = load_scene(&source, Some("time"), Some("latency"), Some("service")).unwrap();

        assert_eq!(scene.series.len(), 2);
        assert_eq!(scene.series[0].name, "api");
        assert_eq!(scene.series[0].points.len(), 2);
        assert_eq!(scene.series[1].name, "worker");
        assert_eq!(scene.series[1].points[0], PlotPoint { x: 3.0, y: 40.0 });
    }

    #[test]
    fn loads_tsv_data() {
        let mut file = NamedTempFile::with_suffix(".tsv").unwrap();
        writeln!(file, "time\tlatency").unwrap();
        writeln!(file, "1\t20").unwrap();
        writeln!(file, "2\t30").unwrap();

        let source = InputSource::from_path(file.path().to_path_buf()).unwrap();
        let scene = load_scene(&source, Some("time"), Some("latency"), None).unwrap();

        assert_eq!(scene.series.len(), 1);
        assert_eq!(scene.series[0].name, "all");
        assert_eq!(scene.series[0].points[1], PlotPoint { x: 2.0, y: 30.0 });
    }

    #[test]
    fn rejects_missing_required_columns() {
        let mut file = NamedTempFile::with_suffix(".csv").unwrap();
        writeln!(file, "time,latency").unwrap();
        writeln!(file, "1,20").unwrap();

        let source = InputSource::from_path(file.path().to_path_buf()).unwrap();

        let missing_y = load_scene(&source, Some("time"), Some("missing"), None);
        let err = missing_y.unwrap_err();
        assert!(err.to_string().contains("missing --y"));
        let missing_x = load_scene(&source, None, Some("latency"), None);
        assert!(missing_x.unwrap_err().to_string().contains("requires --x"));
    }

    #[test]
    fn table_scene_is_bounded() {
        let mut file = NamedTempFile::with_suffix(".csv").unwrap();
        writeln!(file, "time,latency").unwrap();
        for i in 0..(MAX_TABLE_ROWS + 100) {
            writeln!(file, "{i},{}", i * 2).unwrap();
        }

        let source = InputSource::from_path(file.path().to_path_buf()).unwrap();
        let scene = load_scene(&source, Some("time"), Some("latency"), None).unwrap();

        let loaded = scene.series.iter().map(|s| s.points.len()).sum::<usize>();
        assert_eq!(loaded, MAX_TABLE_ROWS);
    }
}
