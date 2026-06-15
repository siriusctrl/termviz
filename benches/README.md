# Local Benchmarks and Smoke Scripts

This directory tracks local benchmark and smoke entrypoints for timing and interaction checks.

## Bench scripts

- `scripts/bench-metadata-inspect.sh`
  - Measures `--inspect` timings for raster, vector, and table inputs.
- `scripts/bench-ansi-export.sh`
  - Measures explicit ANSI export timing for raster assets.
- `scripts/bench-plot-export.sh`
  - Measures explicit plot exports (`json`, `svg`, `ansi`) for CSV input.
- `scripts/bench-interactive-pty.sh`
  - Benchmarks scripted PTY sessions for interactive raster paths.
  - Runs both first-draw and pan/zoom-ish key sequences (`+`, arrow keys, `-`, `0`, `q`) and times each run.
  - Validates that interactive sessions start (alternate screen) and receive scripted input before exiting.

All scripts accept:

- `--quick` for a single iteration per benchmark target.
- `--help` for usage details.

Example:
```bash
./scripts/bench-metadata-inspect.sh --quick
./scripts/bench-ansi-export.sh --quick
./scripts/bench-plot-export.sh --quick
./scripts/bench-interactive-pty.sh --quick
```

Notes:

- The interactive PTY benchmark validates session control behavior under a scripted terminal.
  It does not assert visual correctness; use it as a smoke/latency check, not a
  rendering regression test.

## PTY smoke

- `scripts/smoke-pty.sh`
  - Runs quick interactive image/plot smoke tests under `script` PTY and exits by sending `q`.
  - If `script` is not installed, the check exits 0 with a clear skip message.

Example:
```bash
./scripts/smoke-pty.sh
```

## Release verification helper

- `scripts/release-verify.sh`
  - Runs `cargo fmt --check`, `cargo test`, `cargo clippy`, and `cargo package`.
  - Optionally runs `cargo publish --locked --dry-run` when
    `TERMVIZ_DRY_RUN_PUBLISH=1`.
  - Runs key CLI smoke commands for inspect and export workflows.
