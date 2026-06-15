use assert_cmd::Command;
use predicates::prelude::*;
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

    cmd.assert().failure().stderr(predicate::str::contains(
        "interactive viewing requires a TTY",
    ));
}
