# Changelog

All notable user-facing changes are documented here.

This project follows semantic versioning. Release entries should be used as the
source for GitHub Release notes, so every published version must have a
matching `## [X.Y.Z]` section before the release tag is pushed.

## [Unreleased]

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
  resize redraw, `q` quit, `+`/`-` zoom, `0` fit, `1` actual-ish scale, and
  arrow-key panning.
- Interactive plot viewer for CSV/TSV/JSONL inputs with resize redraw and clear
  validation before raw mode when `--x` or `--y` is missing.
- Terminal rendering backends for ANSI blocks plus Kitty, Sixel, and iTerm2
  raster image protocol payloads. `--protocol auto` detects Kitty and iTerm2
  environment hints and otherwise uses the block fallback.

### Changed

- Redirected stdout remains scriptable by default. Terminal control sequences
  are emitted only for explicit render/export paths or interactive TTY use.

### Known Limitations

- Interactive image viewing currently decodes the full raster before first draw;
  tile-based image readback is still future work.
- Mouse drag panning and a selection/copy-friendly metadata overlay mode are not
  implemented yet.
- The 0.1.0 release process is documented, but actual crates.io publishing and
  release artifact automation remain release tasks until completed.
