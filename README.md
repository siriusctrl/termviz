# termviz

Terminal-first viewing for images and plots.

`termviz` is a young sibling project to `fmtview`: the product shape is a fast,
scriptable CLI that opens rich visual artifacts in a terminal without giving up
large-file discipline or clean stdout behavior.

The first implementation target is intentionally narrow:

```sh
termviz image.png
termviz image.png --format ansi
termviz image.png --format ansi --output frame.ansi
termviz image.png --format png
termviz image.png --format png --output frame.png
termviz image.svg --format svg
termviz data.csv --x time --y latency --format svg
termviz data.csv --x time --y latency --format ansi
termviz data.csv --x time --y latency --format json
termviz data.csv --x time --y latency --kind line
termviz metrics.jsonl --x ts --y value --group service
termviz image.png --inspect
```

## Explicit export

For scriptable workflows, pass `--format`:

```sh
termviz image.png --format json
termviz image.png --format ansi
termviz image.png --format ansi --output frame.ansi
termviz image.png --format png
termviz image.png --format png --output frame.png
termviz image.svg --format svg
termviz data.csv --x ts --y value --format svg
termviz metrics.csv --x ts --y value --format ansi
termviz metrics.csv --x ts --y value --format json
```

`--format json` produces valid JSON with:

- `content`, `shape`, `load`, `render`, `export`, and `plot_kind`
- a `metadata` object with content-specific details
- for plot inputs, a `plot_scene` summary when `--x` and `--y` are provided

`--format ansi` renders a bounded terminal output:

- raster input uses a truecolor half-block fallback,
- CSV/TSV/JSONL input renders a small deterministic ASCII plot based on
  `--x` and `--y` (and `--kind`).

`--format png` writes PNG bytes and supports raster inputs directly.
`--format png` also supports CSV/TSV/JSONL plots when `--x` and `--y` are
provided and uses the same internal plot scene as SVG/ANSI exports.

`--format svg` now works for:

- SVG inputs (copied through unchanged),
- plot inputs (`--x` and `--y` required), rendered as a small deterministic SVG chart.

### Compare same data across outputs

Use the same CSV and plot flags for consistent comparisons:

```sh
termviz examples/latency-demo.csv --x time --y latency --group service
termviz examples/latency-demo.csv --x time --y latency --group service --format svg --output examples/latency-demo.svg
termviz examples/latency-demo.csv --x time --y latency --group service --format png --output examples/latency-demo.png
termviz examples/latency-demo.csv --x time --y latency --group service --format ansi
```

`--output` may be used with `--format ansi`, `--format json`, `--format png`,
and `--format svg` to write results to a file. With no `--output`, the payload
is written to stdout.

Tradeoff: explicit raster exports currently decode the full image before export.
Plot exports use bounded data windows. SVG export from raster inputs is
deferred.

`--inspect` includes asset metadata alongside the resolved profile:

```text
content=Png
shape=RasterImage
load=MetadataFirst
render=TerminalImage
export=ExplicitOnly
plot_kind=none
dimensions=1x1
color=La8
frames=unknown
```

For SVGs, `--inspect` reports a lightweight viewport (from `width`/`height` or
`viewBox`) when it can be found in a bounded header read:

```text
content=Svg
shape=VectorImage
load=RasterizeVector
render=TerminalImage
export=ExplicitOnly
plot_kind=none
viewport=128x64
```

If stdout is a terminal, `termviz` opens an interactive viewer. If stdout is
redirected, it stays scriptable and never emits terminal control sequences
unless the user explicitly asks for that.

For interactive mode:

- Image viewer commands:
  - `q` quit
  - `+` zoom in
  - `-` zoom out
  - `0` fit to terminal
  - `1` set actual-ish scale
  - arrow keys pan across the current rendered image
  - left mouse button drag to pan in terminal cells
  - `m` toggle metadata overlay (file info + render state)
  - window resize redraws immediately
- Plot viewer (`.csv` / `.tsv` / `.jsonl`) loads from `--x` and `--y`, and
  requires both values for interactive viewing.
- Plot viewer:
  - `m` toggles a text summary overlay (points, series, bounds)
- Protocol selection for interactive use:
  - `--protocol auto` currently defaults to blocks unless terminal hints are detected.
  - `--protocol kitty|sixel|iterm|blocks` uses the selected renderer directly for image inputs.

Tradeoff: interactive image mode currently decodes the full image before first
interactive render and does not yet use tile-based readback. Interactive opens are
guarded at a conservative safety threshold of 8,000,000 pixels; larger files now
emit a clear interactive-mode error and must be viewed via `--format` or
`--inspect` unless you reduce size externally.

## Product Boundary

`termviz` is not a dashboard server, notebook runtime, media manager, or
general plotting language. It is a local terminal viewer for visual inspection.

The core promise is:

- Open images and simple plot inputs quickly from a shell.
- Pan, zoom, fit, and inspect visual content in a terminal UI.
- Choose the best terminal image protocol available, with portable fallbacks.
- Keep large images and data streams bounded through metadata-first loading,
  explicit raster guards, and incremental plot data windows.
- Keep redirected stdout useful for metadata, export, or explicit rendering
  requests, never accidental escape-code dumps.

## Architecture Shape

Every input resolves to an `InputProfile` before the runtime chooses how to
load, render, or export it:

```text
  Use case + input
          |
          v
  +-------------------+
  | InputProfile      |
  | - content kind    |
  | - content shape   |
  | - load strategy   |
  | - render strategy |
  | - export policy   |
  +---------+---------+
            |
            v
  +---------+----------+--------------+-------------+
  | asset loading      | plot modeling | TTY viewer  |
  | terminal rendering | export        | inspection  |
  +--------------------+--------------+-------------+
```

See `docs/architecture.md` for the maintainer-facing version of this model.

## Current State

`termviz` has a working first release surface for local terminal viewing and
explicit exports:

```sh
termviz examples/sample.csv --inspect
termviz examples/sample.csv --x time --y latency --format svg
termviz examples/inspect-square.png --format ansi
```

Implemented capabilities include metadata inspection for raster and SVG inputs,
bounded CSV/TSV/JSONL plot loading, explicit JSON/ANSI/PNG/SVG export paths,
ANSI block rendering for raster and plot output, protocol payloads for
interactive raster viewing, and interactive image/plot viewers with keyboard,
mouse, metadata overlay, and resize redraw controls.

Known first-release tradeoffs remain explicit: interactive image viewing decodes
the full raster before first draw after a metadata-based safety guard, and
tile-based image readback is not implemented. npm prebuilt binaries are deferred
for 0.1.0, and publishing to crates.io is still a release task until the package
is actually published.

## TODO

### Milestone 1: Image Inspection Skeleton

- [x] Add fixture images under `examples/`.
- [x] Decode image headers without eagerly decoding full pixel buffers.
- [x] Fill `asset::raster` with metadata-first loading.
- [x] Implement `termviz image.png --inspect` with dimensions, color type, and
      frame count where available.
- [x] Add black-box CLI tests for extension detection and inspect output.

### Milestone 2: Terminal Rendering Backends

- [x] Implement terminal capability detection in `render::terminal`.
- [x] Add Kitty graphics protocol output.
- [x] Add Sixel output.
- [x] Add iTerm2 inline-image output.
- [x] Add ANSI truecolor half-block fallback.
- [x] Keep `--protocol auto|kitty|sixel|iterm|blocks` stable from the start.
- [x] Add protocol snapshot tests that assert escape sequences only appear when
      explicitly requested.

### Milestone 3: Interactive Image Viewer

- [x] Enter raw mode and alternate screen only when stdout is a TTY.
- [ ] Render the first fitted image without decoding more than needed.
- [x] Add pan with arrow keys.
- [x] Add mouse drag.
- [x] Add zoom with `+`, `-`, `0` for fit, and `1` for actual size.
- [x] Add `q` quit.
- [x] Add `m` selection/copy-friendly mode for text metadata overlays.
- [x] Verify viewer behavior in a real PTY.

### Milestone 4: Plot Model

- [x] Add CSV/TSV parsing with bounded row loading.
- [x] Add JSONL parsing with bounded record loading.
- [x] Build a small internal plot scene for line and scatter plots.
- [x] Support `--x`, `--y`, `--group`, and `--kind line|scatter`.
- [x] Render plots to a scene before terminal output.
- [x] Keep unsupported plot inputs as clear CLI errors.

### Milestone 5: Large-File Behavior

- [ ] Introduce tile-based image readback for large rasters.
- [x] Add an interactive raster safety guard before eager decode.
- [x] Add bounded data windows for CSV/TSV/JSONL plot inputs.
- [ ] Add preload hooks for nearby tiles or plot windows.
- [x] Add benchmark scripts for metadata, PTY smoke, and explicit export paths.
- [x] Add benchmark scripts for first draw and scripted pan/zoom interaction.
- [x] Document any whole-file tradeoff explicitly before shipping it.

### Milestone 6: Export and Scriptability

- [x] Add explicit `--output path` export.
- [x] Add `--format ansi` only for explicit export or render commands.
- [x] Add JSON metadata output.
- [x] Add deterministic `--format ansi` for raster and plot paths.
- [x] Keep plain redirected stdout free of terminal control sequences.
- [x] Add tests for TTY versus redirected behavior.
- [x] Add `--format png|svg` export.

### Milestone 7: Packaging and Release

- [x] Add `CHANGELOG.md` entries before each release.
- [x] Add `docs/releasing.md`.
- [x] Add GitHub Actions for fmt, test, and clippy.
- [x] Add release artifact builds.
- [ ] Add crates.io publishing.
- [x] Decide npm scope: npm prebuilt binaries are out of scope for 0.1.0 and
      deferred until package scaffolding and binary installation are implemented.

## Development

```sh
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
```

For terminal UI work, also run the built binary in a real PTY and verify
scrolling, resize, redraw, and quit behavior directly.
