# Changelog

All notable user-facing changes are documented here.

This project follows semantic versioning. Release entries should be used as the
source for GitHub Release notes, so every published version must have a
matching `## [X.Y.Z]` section before the release tag is pushed.

## [Unreleased]

### Added

- Add `--format png` support for CSV/TSV/JSONL plot inputs, rendered from the
  existing `PlotScene` pipeline used by SVG and ANSI exports.
- Add `examples/latency-demo.csv` and prebuilt `examples/latency-demo.svg` and
  `examples/latency-demo.png` outputs for direct side-by-side comparison.

## [0.1.0] - 2026-06-15

### Added

- Initial `termviz` CLI for local terminal-first viewing of images and simple
  plots, with scriptable stdout behavior.
- Inspect mode for raster images and SVGs. `--inspect` reports the resolved
  profile plus raster dimensions, color type, frame count where available, and
  SVG viewport metadata from a bounded header read.
- Explicit export formats:
  - `--format json` for profile and metadata output.
  - `--format ansi` for deterministic ANSI block rendering of raster inputs and
    CSV/TSV/JSONL plots.
  - `--format png` for raster inputs.
  - `--format svg` for SVG inputs and plot inputs.
  - `--output` for writing explicit export payloads to a file.
- Bounded plot scene loading for CSV, TSV, and JSONL inputs, currently capped at
  1024 rows or records. Plot inputs support `--x`, `--y`, `--group`, and
  `--kind line|scatter`.
- Interactive image viewer with raw mode, alternate screen, cleanup on exit,
  resize redraw, `q` quit, `+`/`-` zoom, `0` fit, `1` actual-ish scale,
  arrow-key panning, left-button mouse drag panning, and `m` metadata overlay.
- Interactive plot viewer for CSV/TSV/JSONL inputs with resize redraw, `m`
  summary overlay, and clear validation before raw mode when `--x` or `--y` is
  missing.
- Terminal rendering backends for ANSI blocks plus Kitty, Sixel, and iTerm2
  raster image protocol payloads. `--protocol auto` detects Kitty and iTerm2
  environment hints and otherwise uses the block fallback.
- Linux release-artifact workflow job in CI for tag builds and manual workflow
  dispatch. It uploads a Linux x86_64 tarball plus SHA-256 checksum artifact.
- Local release and benchmark helpers:
  - `scripts/release-verify.sh` for release preflight checks and smoke tests.
  - `scripts/smoke-pty.sh` for basic interactive PTY smoke coverage.
  - `scripts/bench-metadata-inspect.sh`, `scripts/bench-ansi-export.sh`, and
    `scripts/bench-plot-export.sh` for metadata and explicit export timing.
  - `scripts/bench-interactive-pty.sh` for scripted PTY timing of first draw
    and pan/zoom interaction.

### Changed

- Redirected stdout remains scriptable by default. Terminal control sequences
  are emitted only for explicit render/export paths or interactive TTY use.
- Interactive raster launching now guards against very large full-decoded images
  (greater than 8,000,000 pixels) unless an explicit export mode is used.
- Raster profile metadata reports `MetadataFirst` while tile-based raster image
  readback remains a future implementation item.

### Known Limitations

- Interactive image viewing remains eager-decoded and not tile-based; the safety
  guard blocks very large rasters by default and provides a clear CLI error.
- Explicit raster export paths still decode the full image.
- crates.io publishing remains a release task until completed. npm prebuilt
  binaries are deliberately out of scope for 0.1.0.
