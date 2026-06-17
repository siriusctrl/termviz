# Docs Index

Read in this order when getting oriented:

1. `README.md`
2. `AGENTS.md`
3. `CHANGELOG.md`

Read these when the task matches:

- `docs/architecture.md`
  - product boundary
  - input profile model
  - asset loading boundaries
  - terminal render backends
  - plot model boundaries
  - viewer lifecycle
- `docs/releasing.md`
  - release checklist
  - version and tag policy
  - changelog and release notes policy
  - GitHub Release artifacts and checksums
  - `cargo install --git` install path
- `docs/visual-verification.md`
  - real terminal emulator recording
  - fixture batch recordings for plot shapes
  - frame metrics and visible-latency standards
  - PTY recording artifacts
  - keyframe/contact-sheet inspection
  - visual demo handoff expectations
- `docs/testing.md`
  - protocol backend matrix
  - viewer frame matrix
  - selector versus renderer tests
  - CLI/PTY smoke expectations

Code orientation:

- `src/main.rs` is the thin binary entry point.
- `src/lib.rs` exposes the internal modules used by the binary and tests.
- `src/cli.rs` wires CLI arguments to input profile detection, inspection,
  export, and viewer dispatch.
- `src/profile.rs` resolves extensions and future sniffing into an
  `InputProfile`.
- `src/input.rs` owns input materialization from files and, later, stdin.
- `src/input/` owns format-specific sniffing helpers.
- `src/asset.rs` owns the asset-facing entry point.
- `src/asset/` owns image, SVG, animation, and future tile-backed asset readers.
- `src/plot.rs` owns the plot-model entry point.
- `src/plot/` owns CSV/JSONL data parsing and conversion into a small internal
  plot model.
- `src/render.rs` owns render backend selection.
- `src/render/protocols/` owns Kitty and block fallback output.
- `src/render/protocols/plot/` owns calculatable plot rasterization, split into
  theme, layout, text, and target-size raster drawing modules.
- `src/render/protocols/blocks/` owns portable terminal-cell fallbacks, split
  between raster half-block image rendering and plot Braille rendering.
- `src/tui.rs` owns shared terminal session orchestration.
- `src/tui/` owns terminal chrome drawing, plot protocol frame models, palette,
  and layout helpers.
- `src/viewer.rs` owns the shared TTY lifecycle and viewer dispatch.
- `src/viewer/` owns terminal-facing image and plot viewer modes.
- `src/viewer/plot/` owns the interactive plot loop plus focused state, event,
  cache, pan-atlas prefetch, and chrome modules.
- `src/export.rs` owns explicit non-interactive export.
- `tests/cli.rs` covers black-box CLI behavior through the compiled binary.
- `scripts/bench-*.sh` contains local performance entry points. Start with
  `scripts/bench-render-pipeline.sh` for render-stage timing and
  `scripts/bench-plot-e2e.sh` for PTY-observable latency.
- `scripts/record-emulator-demo.sh` records a real Kitty window under Xvfb and
  writes MP4, extracted frames, metrics, keyframes, and a contact sheet.
- `scripts/record-emulator-fixtures.sh` runs the emulator recorder across the
  latency, throughput, error-spike, and scatter/outlier plot fixtures.
- `scripts/record-pty-demo.sh` records raw PTY output for block/TUI inspection.

Keep README user-facing. Keep maintainer-only workflows in docs and link them
from `AGENTS.md`.
