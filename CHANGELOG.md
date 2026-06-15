# Changelog

All notable user-facing changes are documented here.

This project follows semantic versioning. Release entries should be used as the
source for GitHub Release notes, so every published version must have a
matching `## [X.Y.Z]` section before the release tag is pushed.

## [Unreleased]

### Added

- Initialize the `termviz` Rust CLI scaffold with profile detection,
  architecture docs, and TODOs for terminal image and plot viewing.
- Add metadata-first inspect output for raster images and SVGs: `--inspect` now
  reports image dimensions, color type, frame count where available, and SVG
  viewport without eagerly decoding full image data.
- Add explicit non-interactive export foundation with `--format` and profile-aware
  scriptable output:
  - `--format json` now emits valid metadata JSON for raster, SVG, and data inputs.
  - `--format ansi` now renders deterministic terminal output for raster images and
    CSV/TSV/JSONL plots (with `--x`/`--y` required for plotting).
  - `--output` now writes JSON or ANSI payloads to a file when provided.
- Implement Phase D interactive terminal lifecycle and initial viewers:
  - Added TTY lifecycle with alternate screen, raw mode, cursor hiding and cleanup.
  - Added interactive image viewer controls (`q`, `+`, `-`, `0`, `1`, arrow pan) using block
    fallback rendering plus resize redraw.
  - Added interactive plot viewer for CSV/TSV/JSONL inputs with `q` and resize redraw.
  - Missing interactive plot axes now fail clearly before raw mode with `--x`/`--y` required.
  - Non-block `--protocol` values fall back to blocks with a status note.
