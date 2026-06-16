# Testing Matrix

Protocol support is tested in layers. Do not treat one layer as proof for the
others.

## Renderer Backend Tests

The renderer backend tests live beside the renderer modules:

- `src/render/protocols/blocks/`
- `src/render/protocols/kitty.rs`
- `src/render/protocols/sixel.rs`
- `src/render/protocols/iterm.rs`
- `src/render/protocols/mod.rs`

These tests verify payload structure, sizing metadata, chunking, alpha handling,
fallback markers, and backend dispatch. They do not prove that a real terminal
will render a pixel protocol successfully.

## Viewer Frame Tests

Viewer frame tests live in:

- `src/viewer/image.rs`
- `src/viewer/plot.rs`

They verify that image inputs and calculatable plot scenes can render through
every explicit protocol: `blocks`, `kitty`, `sixel`, and `iterm`.
Pixel-protocol plot tests should decode at least one payload and assert that
the embedded image uses the requested target size, not the fixed export size.
Plot viewer tests should also cover frame-cache reuse, resize cache misses, and
large-window target capping for protocols that can scale payloads to a requested
cell area.

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
includes decoded payload bytes and total PTY stream bytes so protocol overhead
is visible.

## Selector Tests

Selector tests live in `src/render/terminal.rs`.

`auto` is a selector, not a renderer. Its tests cover environment-hint
selection and conservative tmux/screen fallback. A future true terminal query
probe should add tests here without weakening explicit protocol tests.

## CLI and PTY Tests

CLI tests live in `tests/cli.rs`.

They cover scriptable stdout, export behavior, idle redraw behavior, and a PTY
protocol matrix that starts the viewer with every explicit protocol and checks
that the expected status line and payload marker are emitted.

## Visual Recording

For visual TUI changes, run:

```sh
scripts/record-pty-demo.sh target/termviz-recordings/<name> -- target/debug/termviz examples/latency-demo.csv --x time --y latency --group service
```

Then inspect `contact-sheet.png` or `keyframes/` before reporting completion.
The recording artifacts are evidence for block/TUI visuals and can also be used
as product demos.

Pixel protocols such as Kitty, iTerm2, and Sixel still need payload-level
tests unless the current environment provides a real terminal screenshot or
screen recording for that protocol.
For calculatable plot changes, decode at least one pixel-protocol payload and
assert the embedded image size and background color match the interactive
target, since PTY capture by itself only proves that escape data was emitted.
For resize changes, prefer a raw `tmux pipe-pane` capture for pixel protocols so
the actual image payloads can be decoded before and after the resize.
