# termviz

Terminal-first viewing for images and plots.

`termviz` is a young sibling project to `fmtview`: the product shape is a fast,
scriptable CLI that opens rich visual artifacts in a terminal without giving up
large-file discipline or clean stdout behavior.

The first implementation target is intentionally narrow:

```sh
termviz image.png
termviz data.csv --x time --y latency --kind line
termviz metrics.jsonl --x ts --y value --group service
termviz image.png --inspect
```

If stdout is a terminal, `termviz` should open an interactive viewer. If stdout
is redirected, it should stay scriptable and never emit terminal control
sequences unless the user explicitly asks for that.

## Product Boundary

`termviz` is not a dashboard server, notebook runtime, media manager, or
general plotting language. It is a local terminal viewer for visual inspection.

The core promise is:

- Open images and simple plot inputs quickly from a shell.
- Pan, zoom, fit, and inspect visual content in a terminal UI.
- Choose the best terminal image protocol available, with portable fallbacks.
- Keep large images and data streams bounded through metadata-first loading,
  tiles, frames, or incremental plot data windows.
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

This repository is initialized as a handoff-ready scaffold. The CLI compiles
and can inspect input profiles, but the actual image viewer, plot renderer, and
terminal protocol implementations are TODO.

```sh
termviz examples/sample.csv --inspect
```

## TODO

### Milestone 1: Image Inspection Skeleton

- [ ] Add fixture images under `examples/`.
- [ ] Decode image headers without eagerly decoding full pixel buffers.
- [ ] Fill `asset::raster` with metadata-first loading.
- [ ] Implement `termviz image.png --inspect` with dimensions, color type, and
      frame count where available.
- [ ] Add black-box CLI tests for extension detection and inspect output.

### Milestone 2: Terminal Rendering Backends

- [ ] Implement terminal capability detection in `render::terminal`.
- [ ] Add Kitty graphics protocol output.
- [ ] Add Sixel output.
- [ ] Add iTerm2 inline-image output.
- [ ] Add ANSI truecolor half-block fallback.
- [ ] Keep `--protocol auto|kitty|sixel|iterm|blocks` stable from the start.
- [ ] Add protocol snapshot tests that assert escape sequences only appear when
      explicitly requested.

### Milestone 3: Interactive Image Viewer

- [ ] Enter raw mode and alternate screen only when stdout is a TTY.
- [ ] Render the first fitted image without decoding more than needed.
- [ ] Add pan with arrow keys and mouse drag.
- [ ] Add zoom with `+`, `-`, `0` for fit, and `1` for actual size.
- [ ] Add `q` quit.
- [ ] Add `m` selection/copy-friendly mode for text metadata overlays.
- [ ] Verify viewer behavior in a real PTY.

### Milestone 4: Plot Model

- [ ] Add CSV sniffing and column metadata.
- [ ] Add JSONL streaming data sniffing.
- [ ] Build a small internal plot model for line and scatter plots.
- [ ] Support `--x`, `--y`, `--group`, and `--kind line|scatter`.
- [ ] Render plots to an internal raster or scene before terminal output.
- [ ] Keep unsupported plot inputs as clear CLI errors.

### Milestone 5: Large-File Behavior

- [ ] Introduce tile-based image readback for large rasters.
- [ ] Add bounded data windows for CSV and JSONL plot inputs.
- [ ] Add preload hooks for nearby tiles or plot windows.
- [ ] Add benchmark scripts for first draw, pan redraw, zoom redraw, and export.
- [ ] Document any whole-file tradeoff explicitly before shipping it.

### Milestone 6: Export and Scriptability

- [ ] Add explicit `--output path` export.
- [ ] Add `--format png|svg|ansi` only for explicit export or render commands.
- [ ] Add `--json` metadata output.
- [ ] Keep plain redirected stdout free of terminal control sequences.
- [ ] Add tests for TTY versus redirected behavior.

### Milestone 7: Packaging and Release

- [ ] Add `CHANGELOG.md` entries before each release.
- [ ] Add `docs/releasing.md`.
- [ ] Add GitHub Actions for fmt, test, clippy, and release artifacts.
- [ ] Add crates.io publishing.
- [ ] Decide whether npm should ship prebuilt Linux binaries like `fmtview`.

## Development

```sh
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
```

For terminal UI work, also run the built binary in a real PTY and verify
scrolling, resize, redraw, and quit behavior directly.
