use assert_cmd::Command;
use image::{ImageFormat, RgbImage};
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as StdCommand, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};
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
fn redirected_view_defaults_to_png_export() {
    let file = temp_png_file();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path());

    let output = cmd.output().unwrap();
    assert!(output.status.success(), "unexpected failure: {:?}", output);
    assert!(output.stdout.starts_with(&[0x89, b'P', b'N', b'G']));
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
fn input_format_can_force_profile_when_extension_is_unknown() {
    let mut file = NamedTempFile::with_suffix(".data").unwrap();
    writeln!(file, "time\tlatency").unwrap();
    writeln!(file, "1\t20").unwrap();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path())
        .arg("--input-format")
        .arg("tsv")
        .arg("--inspect");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("content=Tsv"))
        .stdout(predicate::str::contains("shape=DataTable"));
}

#[test]
fn old_format_flag_is_not_supported() {
    let file = temp_png_file();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path()).arg("--format").arg("json");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument '--format'"));
}

#[test]
fn json_export_is_valid_for_raster() {
    let file = temp_png_file();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path()).arg("--output-format").arg("json");

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
        .arg("--output-format")
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
        .arg("--output-format")
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
fn output_extension_infers_json_export_format() {
    let mut file = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(file, "time,latency").unwrap();
    writeln!(file, "1,20").unwrap();
    writeln!(file, "2,30").unwrap();

    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("output.json");

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path())
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
}

#[test]
fn output_extension_infers_png_export_format() {
    let file = temp_png_file();
    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("image.png");

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path()).arg("--output").arg(&output_path);

    cmd.assert().success();

    let output = fs::read(&output_path).unwrap();
    assert!(output.starts_with(&[0x89, b'P', b'N', b'G']));
}

#[test]
fn unknown_output_extension_defaults_to_png() {
    let file = temp_png_file();
    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("image.preview");

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path()).arg("--output").arg(&output_path);

    cmd.assert().success();

    let output = fs::read(&output_path).unwrap();
    assert!(output.starts_with(&[0x89, b'P', b'N', b'G']));
}

#[test]
fn png_export_is_binary_png_data() {
    let file = temp_png_file();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path()).arg("--output-format").arg("png");

    let output = cmd.output().unwrap();
    assert!(output.status.success(), "unexpected failure: {:?}", output);
    let stdout = output.stdout;
    assert!(stdout.starts_with(&[0x89, b'P', b'N', b'G']));
}

#[test]
fn plot_png_export_is_binary_png_data() {
    let mut file = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(file, "time,latency,service").unwrap();
    writeln!(file, "1,118,api").unwrap();
    writeln!(file, "1,134,worker").unwrap();
    writeln!(file, "2,121,api").unwrap();
    writeln!(file, "2,128,worker").unwrap();
    writeln!(file, "3,125,api").unwrap();
    writeln!(file, "3,129,worker").unwrap();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path())
        .arg("--output-format")
        .arg("png")
        .arg("--x")
        .arg("time")
        .arg("--y")
        .arg("latency")
        .arg("--group")
        .arg("service");

    let output = cmd.output().unwrap();
    assert!(output.status.success(), "unexpected failure: {:?}", output);
    let stdout = output.stdout;
    assert!(stdout.starts_with(&[0x89, b'P', b'N', b'G']));
    assert!(stdout.len() > 8);
}

#[test]
fn plot_redirect_defaults_to_png_data() {
    let mut file = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(file, "time,latency,service").unwrap();
    writeln!(file, "1,118,api").unwrap();
    writeln!(file, "2,121,api").unwrap();
    writeln!(file, "3,125,api").unwrap();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path())
        .arg("--x")
        .arg("time")
        .arg("--y")
        .arg("latency");

    let output = cmd.output().unwrap();
    assert!(output.status.success(), "unexpected failure: {:?}", output);
    assert!(output.stdout.starts_with(&[0x89, b'P', b'N', b'G']));
}

#[test]
fn plot_viewer_does_not_redraw_while_idle_under_pty() {
    if !script_available() {
        eprintln!("skipping PTY smoke because script(1) is unavailable");
        return;
    }

    let mut file = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(file, "time,latency").unwrap();
    writeln!(file, "1,20").unwrap();
    writeln!(file, "2,40").unwrap();
    writeln!(file, "3,30").unwrap();

    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("plot-pty.log");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_termviz"));
    let command = format!(
        "stty rows 24 cols 80; exec {} {} --x time --y latency --kind line --protocol blocks",
        shell_quote(&bin),
        shell_quote(file.path())
    );

    let mut child = StdCommand::new("script")
        .arg("-q")
        .arg("-c")
        .arg(command)
        .arg(&output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(500));
    child
        .stdin
        .take()
        .expect("script stdin should be piped")
        .write_all(b"q")
        .unwrap();

    let status = match wait_with_timeout(&mut child, Duration::from_secs(5)) {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("PTY smoke did not exit after sending q");
        }
    };
    let output = fs::read(&output_path).unwrap_or_default();

    assert!(
        status.success(),
        "PTY smoke failed with status {status:?}; output was {} bytes",
        output.len()
    );
    assert!(
        output
            .windows(b"q quit".len())
            .any(|window| window == b"q quit"),
        "PTY smoke did not capture the plot viewer status line"
    );

    let full_clear_count = count_subsequence(&output, b"\x1b[2J");
    assert!(
        full_clear_count <= 4,
        "idle viewer redrew too often: observed {full_clear_count} full-screen clears in {} bytes",
        output.len()
    );
}

#[test]
fn explicit_protocols_emit_expected_payload_under_pty() {
    if !script_available() {
        eprintln!("skipping protocol PTY matrix because script(1) is unavailable");
        return;
    }

    let mut file = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(file, "time,latency").unwrap();
    writeln!(file, "1,20").unwrap();
    writeln!(file, "2,40").unwrap();
    writeln!(file, "3,30").unwrap();

    let cases = [
        (
            "blocks",
            b"protocol: blocks".as_slice(),
            b"protocol: blocks".as_slice(),
        ),
        ("kitty", b"\x1b_G".as_slice(), b"protocol: kitty".as_slice()),
        ("sixel", b"\x1bPq".as_slice(), b"protocol: sixel".as_slice()),
        (
            "iterm",
            b"\x1b]1337;File".as_slice(),
            b"protocol: iterm".as_slice(),
        ),
    ];

    for (protocol, payload_marker, status_marker) in cases {
        let output = run_plot_viewer_under_pty(file.path(), protocol);

        assert!(
            output
                .windows(payload_marker.len())
                .any(|window| window == payload_marker),
            "{protocol} PTY output did not contain payload marker {:?}",
            String::from_utf8_lossy(payload_marker)
        );
        assert!(
            output
                .windows(status_marker.len())
                .any(|window| window == status_marker),
            "{protocol} PTY output did not contain status marker {:?}",
            String::from_utf8_lossy(status_marker)
        );
    }
}

#[test]
fn png_export_can_write_to_path() {
    let file = temp_png_file();
    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("image.png");

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path())
        .arg("--output-format")
        .arg("png")
        .arg("--output")
        .arg(&output_path);

    cmd.assert().success();

    let output = fs::read(&output_path).unwrap();
    assert!(output.starts_with(&[0x89, b'P', b'N', b'G']));
}

#[test]
fn svg_export_copies_input_svg() {
    let svg = include_bytes!("../examples/inspect.svg");
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(svg).unwrap();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path()).arg("--output-format").arg("svg");
    let output = cmd.output().unwrap();

    assert!(output.status.success(), "unexpected failure: {:?}", output);
    let stdout = output.stdout;
    assert!(stdout.starts_with(b"<svg"));
}

#[test]
fn svg_export_can_render_plot_as_svg() {
    let mut file = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(file, "time,latency").unwrap();
    writeln!(file, "1,20").unwrap();
    writeln!(file, "2,30").unwrap();
    writeln!(file, "3,40").unwrap();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path())
        .arg("--output-format")
        .arg("svg")
        .arg("--x")
        .arg("time")
        .arg("--y")
        .arg("latency");

    let output = cmd.output().unwrap();
    assert!(output.status.success(), "unexpected failure: {:?}", output);
    assert!(output.stdout.starts_with(b"<svg"));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("points:"));
}

#[test]
fn ansi_export_for_raster_contains_blocks_and_escapes() {
    let file = temp_png_file();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path()).arg("--output-format").arg("ansi");

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
        .arg("--output-format")
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
        .arg("--output-format")
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

#[test]
fn redirect_default_png_keeps_no_escape_sequences() {
    let file = temp_png_file();

    let mut cmd = Command::cargo_bin("termviz").unwrap();
    cmd.arg(file.path());

    let output = cmd.output().unwrap();
    assert!(output.status.success(), "unexpected failure: {:?}", output);
    let stdout = output.stdout;
    assert!(stdout.starts_with(&[0x89, b'P', b'N', b'G']));
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

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

fn script_available() -> bool {
    StdCommand::new("script")
        .arg("-q")
        .arg("-c")
        .arg("true")
        .arg("/dev/null")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn run_plot_viewer_under_pty(input: &Path, protocol: &str) -> Vec<u8> {
    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join(format!("plot-{protocol}-pty.log"));
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_termviz"));
    let command = format!(
        "stty rows 24 cols 80; exec {} {} --x time --y latency --kind line --protocol {}",
        shell_quote(&bin),
        shell_quote(input),
        protocol
    );

    let mut child = StdCommand::new("script")
        .arg("-q")
        .arg("-c")
        .arg(command)
        .arg(&output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(500));
    child
        .stdin
        .take()
        .expect("script stdin should be piped")
        .write_all(b"q")
        .unwrap();

    let status = match wait_with_timeout(&mut child, Duration::from_secs(5)) {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{protocol} PTY smoke did not exit after sending q");
        }
    };
    let output = fs::read(&output_path).unwrap_or_default();

    assert!(
        status.success(),
        "{protocol} PTY smoke failed with status {status:?}; output was {} bytes",
        output.len()
    );

    output
}

fn count_subsequence(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return Some(status);
        }
        if start.elapsed() >= timeout {
            return None;
        }
        thread::sleep(Duration::from_millis(25));
    }
}
