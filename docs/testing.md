# Testing Matrix

Protocol support is tested in layers. Do not treat one layer as proof for the
others.

## Renderer Backend Tests

The renderer backend tests live beside the renderer modules:

- `src/render/protocols/blocks/`
- `src/render/protocols/kitty.rs`
- `src/render/protocols/mod.rs`

These tests verify payload structure, sizing metadata, chunking, alpha handling,
fallback markers, and backend dispatch. They do not prove that a real terminal
will render a pixel protocol successfully.

## Viewer Frame Tests

Viewer frame tests live in:

- `src/viewer/image.rs`
- `src/viewer/plot.rs`

They verify that image inputs and calculatable plot scenes can render through
both explicit protocols: `blocks` and `kitty`. Kitty plot tests should decode at
least one payload and assert that the embedded image uses the interactive raster
target, not the fixed export size. Tests should also assert requested terminal
cell placement. Plot viewer tests should cover frame-cache reuse, resize cache
misses, and large-window target capping for Kitty frames.

The interactive plot recompute path has a local perf test:

```sh
scripts/bench-plot-recompute.sh --quick
```

It wraps the ignored `plot_recompute_perf` Rust test and reports CSV metrics for
the recompute path without requiring a terminal session. The output includes
mean payload bytes so rendering cost and output size can be compared.

The action-to-terminal-output path has a local PTY perf test:

```sh
scripts/bench-plot-e2e.sh --quick
```

It reports timings from scripted key/resize actions to complete Kitty payloads
arriving on the PTY stream. This catches app-side redraw and protocol-output
latency, but not terminal compositor or physical display scanout. The output
includes decoded PNG payload bytes and total PTY stream bytes so protocol
overhead is visible. The benchmark drives a direct PTY and avoids tmux
passthrough ambiguity, but the millisecond timings still stop at PTY-observable
bytes rather than at terminal GPU composition or physical display scanout.

## Selector Tests

Selector tests live in `src/render/terminal.rs`.

`auto` is a selector, not a renderer. Its tests cover environment-hint
selection, Kitty-compatible hints inside tmux/screen, and conservative
multiplexer fallback when no outer-terminal hint is visible. A future true
terminal query probe should add tests here without weakening explicit protocol
tests.

## CLI and PTY Tests

CLI tests live in `tests/cli.rs`.

They cover scriptable stdout, export behavior, idle redraw behavior, and a PTY
protocol matrix that starts the viewer with every explicit protocol and checks
that the expected styled status-bar label and payload marker are emitted.

## Visual Recording

For visual TUI changes, run:

```sh
scripts/record-pty-demo.sh target/termviz-recordings/<name> -- target/debug/termviz examples/latency-demo.csv --x time --y latency --group service
```

Then inspect `contact-sheet.png` or `keyframes/` before reporting completion.
The recording artifacts are evidence for block/TUI visuals and can also be used
as product demos.

The contact sheet is rendered from captured text frames, so ANSI colors in the
styled status bar are easier to inspect in `frames/*.ansi` or by replaying the
session. Use the sheet for layout, clipping, and blank-screen checks, and the
ANSI frame for status-bar color/background checks.

Kitty still needs payload-level tests unless the current environment provides a
real terminal screenshot or screen recording for that protocol.
For calculatable plot changes, decode at least one pixel-protocol payload and
assert the embedded image size and background color match the interactive
target, since PTY capture by itself only proves that escape data was emitted.
For resize changes, prefer a raw `tmux pipe-pane` capture for pixel protocols so
the actual image payloads can be decoded before and after the resize.
