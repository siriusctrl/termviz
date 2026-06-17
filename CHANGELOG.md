# Changelog

All notable user-facing changes are documented here.

This project follows semantic versioning. Release entries should be used as the
source for GitHub Release notes, so every published version must have a
matching `## [X.Y.Z]` section before the release tag is pushed.

## [Unreleased]

### Added

- Add a real terminal emulator recording helper that runs Kitty on Xvfb,
  captures MP4 plus extracted frames, and reports visible latency, blank-frame,
  and large-delta metrics for visual regression checks.
- Add CSV fixtures for throughput, error spikes, and scatter/outlier plots plus
  a fixture wrapper for batch emulator recordings.

### Changed

- Refresh the interactive plot viewer visual treatment with a calmer dark
  palette, clearer series colors, thicker interactive line strokes, larger
  scatter marks, cleaner plot chrome, and consistent control-bar wording.

### Fixed

- Draw Kitty plot image payloads after terminal chrome on full chrome repaints
  so the first recorded frame includes the chart body instead of only axes and
  labels.
- Stabilize emulator recording action metrics so fast baseline-frame updates
  and post-quit shell redraws do not skew visible-latency results.

## [0.2.1] - 2026-06-16

### Changed

- Simplify releases to GitHub Release artifacts plus `cargo install --git`.
  crates.io and npm publishing are no longer part of the release path.

## [0.2.0] - 2026-06-16

### Added

- Add a tag-driven release workflow that builds a static Linux x64 artifact,
  publishes GitHub Release assets, and validates the Rust package locally.
- Default redirected/export output to PNG when no `--output-format` is provided,
  while still inferring JSON/ANSI/SVG/PNG from optional `--output` extensions
  when they are recognized.
- Add `--input-format` for forcing input type when extension and bounded
  sniffing are ambiguous or wrong.
- Add `--output-format png` support for CSV/TSV/JSONL plot inputs, rendered from the
  existing `PlotScene` pipeline used by SVG and ANSI exports.
- Add `examples/latency-demo.csv` and prebuilt `examples/latency-demo.svg` and
  `examples/latency-demo.png` outputs for direct side-by-side comparison.
- Add interactive plot viewport controls (`←/→/↑/↓`, `+/-`, `0`, `m`) with a
  structured status line and full data-range fit mode indicator.
- Render interactive plot visuals with axis labels, a textual legend, and
  visible-range-aware line/scatter rendering.
- Add deterministic plot visual signature tests for the export PNG and
  interactive dark PNG paths, plus display-list clipping coverage for
  viewport-crossing line segments.
- Add antialiased monospace text rendering for plot PNG and pixel-protocol plot
  frames, with a built-in bitmap fallback for minimal Linux environments.
- Add `scripts/bench-render-pipeline.sh` for unified image and plot render-stage
  metrics across Kitty and Blocks.

### Changed

- Plot interactive UI now draws a structured chart instead of the old ASCII
  marker-only viewport, while keeping scriptable export and interactive TTY safety.
- Interactive plot viewing now follows the calculatable-scene path: Kitty
  renders the current plot viewport for the active terminal shape and dark
  viewer theme, while blocks remains the terminal-cell fallback.
- Interactive plot viewing now coalesces pending key/resize events, reuses
  unchanged frame payloads, and avoids full-screen clears for image protocol
  frames to reduce input lag and resize flicker.
- Kitty plot viewing still fills the resized terminal cell area, but caps very
  large internal raster targets to keep redraw and PNG encoding cost bounded.
- Kitty interactive plot frames keep remote-safe direct-data payloads while
  preserving full internal raster size for normal terminal windows.
- Interactive plot block rendering now uses a dark terminal-native Braille view
  with smoother plot lines, softer axes, and terminal-friendly series colors.
- Kitty interactive raster frames now request the active terminal cell size, so
  fitted images and rasterized plot scenes fill the viewer area instead of
  appearing as tiny source-pixel payloads in the top-left corner.
- `--protocol auto` now prefers Kitty-compatible terminal hints, including
  Kitty, WezTerm, and Ghostty, before falling back to blocks.
- Interactive image fit mode now renders through a dark, terminal-shaped canvas
  so transparent images keep the dark viewer feel and fitted images preserve
  aspect ratio instead of being stretched to the terminal rectangle.
- Interactive image and plot viewers now draw a styled segmented status bar
  instead of a plain debug-style status string.
- Interactive pixel-protocol plot viewing now draws file path, legend, axis
  labels, and controls as real terminal text around a smaller body-only image
  payload, keeping chrome crisp and reducing payload bytes. The bottom bar is
  now control-only so protocol and plot metadata are not repeated.
- `--protocol auto` now recognizes Ghostty by `TERM=xterm-ghostty` and keeps
  Kitty-compatible terminals on the Kitty path even when tmux/screen changes
  `TERM`, falling back to blocks only when no reliable outer-terminal hint is
  visible.
- Add `scripts/record-pty-demo.sh` for repeatable terminal visual recordings of
  PTY smoke sessions.
- Add `scripts/bench-plot-recompute.sh` for local timing of the interactive plot
  recompute pipeline without starting a terminal.
- Expand the plot recompute benchmark with display-list, rasterization,
  protocol-encoding, payload-byte, command-count, and image-pixel columns.
- Expand render perf coverage with profile/load, layout, compose, terminal
  chrome, payload-byte, command-count, and image-pixel columns.
- Add `scripts/bench-plot-e2e.sh` for local PTY timing from scripted plot
  actions to terminal-observable Kitty payload output.
- PTY recordings now emit raw frames, keyframe PNGs, a contact sheet, manifest,
  and inspection summary so visual output can be reviewed as both verification
  evidence and product demo material.
- Add a documented protocol testing matrix covering renderer backends, viewer
  frames, `auto` selector behavior, and CLI/PTY smoke checks.
- Share plot layout, clipping, visible mark generation, and dense-line
  downsampling through an internal display list used by PNG, SVG, and
  pixel-protocol plot rendering.
- Optimize Kitty image and plot render hot paths by keeping interactive frames
  in RGBA form through PNG encoding, caching resized image frames across pans,
  and replacing generic dark-matte overlay work with a byte-equivalent
  terminal-viewer compositor.
- Reduce Kitty interaction flicker by keeping image placements from moving the
  terminal cursor and by updating plot pixel payloads before repainting the
  surrounding terminal chrome.
- Reduce plot pan/zoom output size by sending Kitty plot frames as zlib-compressed
  raw RGBA payloads instead of PNG payloads, and by skipping static plot chrome
  repaint work on same-size interaction frames.

### Removed

- Remove interactive Sixel and iTerm2 backends. Interactive rendering now
  supports Kitty-compatible terminals plus the ANSI/Braille blocks fallback.

### Fixed

- Render interactive plot block output without raster half-block bands, wide
  glyph wrapping, or raw-mode line-feed drift.
- Stop interactive image and plot viewers from redrawing full frames during idle
  poll timeouts; frames now redraw only after input, resize, or state changes.
- Emit Kitty graphics payloads with protocol-compliant direct-data chunk sizes
  and continuation headers.
- Cover every explicit interactive protocol (`blocks` and `kitty`) in renderer
  dispatch, viewer frame, and CLI/PTY tests.
- Render interactive pixel-protocol plot frames with the dark viewer theme
  instead of reusing the white-background export theme.
- Split calculatable plot rasterization into theme, layout, text, and raster
  modules, with interactive rendering no longer scaling a fixed 640x360 export
  image.
- Move plot SVG export out of the data model and onto the shared render path so
  SVG and PNG exports use the same chart layout, axes, legend, and clipped
  series geometry.
- Split the portable block fallback into separate raster-image and plot fallback
  modules.

## [0.1.0] - 2026-06-15

### Added

- Initial `termviz` CLI for local terminal-first viewing of images and simple
  plots, with scriptable stdout behavior.
- Inspect mode for raster images and SVGs. `--inspect` reports the resolved
  profile plus raster dimensions, color type, frame count where available, and
  SVG viewport metadata from a bounded header read.
- Explicit export formats:
  - `--output-format json` for profile and metadata output.
  - `--output-format ansi` for deterministic ANSI block rendering of raster inputs and
    CSV/TSV/JSONL plots.
  - `--output-format png` for raster inputs.
  - `--output-format svg` for SVG inputs and plot inputs.
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
- Registry publishing is not part of the release path. Install from GitHub with
  `cargo install --git`.
