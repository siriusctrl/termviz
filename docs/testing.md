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
Sized Kitty payloads should include `C=1` so image placement does not move the
terminal cursor after a full-width/full-height frame. Without that, zoom and pan
updates can leave the cursor at an implementation-defined edge position and
produce visible flicker in compatible terminals.

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

The full render pipeline has a local perf script:

```sh
scripts/bench-render-pipeline.sh --quick
```

It wraps ignored Rust perf tests for image and plot rendering and reports one
CSV schema for Kitty and Blocks. The output splits profile/load, layout,
display-list, raster/resize, compose, protocol encoding, terminal chrome,
payload bytes, command count, and image pixels. Image coverage includes a
Kitty hot-pan metric so resize-cache changes can be measured separately from
first-frame decode and resize work. This is the first script to run when a
render change might affect responsiveness or output size.

The interactive plot recompute path also has a smaller local perf test:

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
includes decoded lossless image payload bytes, including PNG and raw RGBA Kitty
frames, plus total PTY stream bytes so protocol overhead is visible. The
benchmark drives a direct PTY and avoids tmux passthrough ambiguity, but the
millisecond timings still stop at PTY-observable bytes rather than at terminal
GPU composition or physical display scanout.

The `*_prefetched` metrics wait briefly between repeated navigation actions.
They measure whether the direction-biased encoded-frame cache and pan prefetch
path are actually helping repeated `+` and arrow-key interactions, separate
from the uncached first keypress metrics. For Kitty, a healthy prefetched
navigation hit should show `payload_bytes_delta` near zero because the
foreground update is an image placement command for an idle-transmitted image,
not a new image transfer.

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
For image compositor changes, keep byte-level tests against the previous
dark-matte alpha behavior; small alpha differences change Kitty PNG payloads
and can show up as visual regressions on translucent inputs.
For resize changes, prefer a raw `tmux pipe-pane` capture for pixel protocols so
the actual image payloads can be decoded before and after the resize.
