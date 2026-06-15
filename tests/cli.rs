use assert_cmd::Command;
use image::{ImageFormat, RgbImage};
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn inspect_reports_csv_profile() {
    let mut file = NamedTempFile::with_suffix(".csv").unwrap();
    std::io::Write::write_all(&mut file, b"time,latency\n1,20\n").unwrap();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path()).arg("--inspect");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("content=Csv"))
        .stdout(predicate::str::contains("shape=DataTable"))
        .stdout(predicate::str::contains("render=TerminalPlot"));
}

#[test]
fn redirected_view_without_inspect_is_an_error() {
    let mut file = NamedTempFile::with_suffix(".png").unwrap();
    std::io::Write::write_all(&mut file, b"placeholder").unwrap();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path());

    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stderr.contains("interactive viewing requires a TTY"));
    assert!(!stdout.contains('\u{1b}'));
    assert_eq!(stdout, "");
}

#[test]
fn inspect_reports_png_metadata() {
    let file = temp_png_file();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path()).arg("--inspect");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("content=Png"))
        .stdout(predicate::str::contains("dimensions=2x2"))
        .stdout(predicate::str::contains("color="))
        .stdout(predicate::str::contains("frames="));
}

#[test]
fn inspect_reports_svg_viewport() {
    let svg = include_bytes!("../examples/inspect.svg");
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(svg).unwrap();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path()).arg("--inspect");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("content=Svg"))
        .stdout(predicate::str::contains("viewport=128x64"));
}

#[test]
fn json_export_is_valid_for_raster() {
    let file = temp_png_file();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path()).arg("--format").arg("json");

    let output = cmd.output().unwrap();
    assert!(output.status.success(), "unexpected failure: {:?}", output);

    let stdout = String::from_utf8(output.stdout).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(payload["content"], "Png");
    assert_eq!(payload["shape"], "RasterImage");
    assert_eq!(payload["metadata"]["content_type"], "raster");
    assert!(
        payload["metadata"]["dimensions"]["available"]
            .as_bool()
            .unwrap_or(false)
    );
}

#[test]
fn json_export_profile_only_when_xy_missing_for_data() {
    let mut file = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(file, "time,latency").unwrap();
    writeln!(file, "1,20").unwrap();
    writeln!(file, "2,30").unwrap();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path())
        .arg("--format")
        .arg("json")
        .arg("--kind")
        .arg("line");

    let output = cmd.output().unwrap();
    assert!(output.status.success(), "unexpected failure: {:?}", output);

    let stdout = String::from_utf8(output.stdout).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["content"], "Csv");
    assert!(
        !payload["metadata"]["plot_scene"]["loaded"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(
        payload["metadata"]["plot_scene"]["reason"],
        "x and y required for plot scene loading"
    );
}

#[test]
fn json_export_can_write_to_path() {
    let mut file = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(file, "time,latency").unwrap();
    writeln!(file, "1,20").unwrap();
    writeln!(file, "2,30").unwrap();

    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("output.json");

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path())
        .arg("--format")
        .arg("json")
        .arg("--x")
        .arg("time")
        .arg("--y")
        .arg("latency")
        .arg("--output")
        .arg(&output_path);

    cmd.assert().success();

    let output = fs::read_to_string(&output_path).unwrap();
    let payload: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(payload["content"], "Csv");
    assert_eq!(payload["metadata"]["plot_scene"]["loaded"], true);
    assert!(
        payload["metadata"]["plot_scene"]["series"]
            .as_array()
            .is_some()
    );
}

#[test]
fn ansi_export_for_raster_contains_blocks_and_escapes() {
    let file = temp_png_file();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path()).arg("--format").arg("ansi");

    let output = cmd.output().unwrap();
    assert!(output.status.success(), "unexpected failure: {:?}", output);

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains('\u{1b}'));
    assert!(stdout.contains('▀'));
    assert!(stdout.contains("\x1b[0m"));
}

#[test]
fn ansi_export_can_write_to_path() {
    let file = temp_png_file();
    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("frame.ansi");

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path())
        .arg("--format")
        .arg("ansi")
        .arg("--output")
        .arg(&output_path);

    cmd.assert().success();

    let output = fs::read_to_string(&output_path).unwrap();
    assert!(output.contains('\u{1b}'));
    assert!(output.contains("\x1b[0m"));
}

#[test]
fn ansi_export_for_plot_contains_axes() {
    let mut file = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(file, "time,latency").unwrap();
    writeln!(file, "1,20").unwrap();
    writeln!(file, "2,40").unwrap();
    writeln!(file, "3,30").unwrap();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path())
        .arg("--format")
        .arg("ansi")
        .arg("--x")
        .arg("time")
        .arg("--y")
        .arg("latency");

    let output = cmd.output().unwrap();
    assert!(output.status.success(), "unexpected failure: {:?}", output);

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains('+'));
    assert!(stdout.contains('-'));
    assert!(stdout.contains('|'));
}

fn temp_png_file() -> NamedTempFile {
    let file = NamedTempFile::with_suffix(".png").unwrap();
    let image =
        RgbImage::from_vec(2, 2, vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]).unwrap();
    image::DynamicImage::ImageRgb8(image)
        .save_with_format(file.path(), ImageFormat::Png)
        .unwrap();
    file
}
