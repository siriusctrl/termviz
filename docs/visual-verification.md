# Visual Verification

Terminal UI changes should leave visual evidence, not just command output.

## Real Terminal Recording

Use the emulator recording helper for Kitty or any change where the final
terminal-composited image matters:

```sh
scripts/record-emulator-demo.sh target/termviz-emulator-recordings/<name> -- target/debug/termviz examples/latency-demo.csv --x time --y latency --group service
```

The helper starts a real Kitty window on an Xvfb display, records the screen,
sends a fixed sequence of `+`, arrow, `-`, `0`, and `q` keys with `xdotool`,
extracts PNG frames from the MP4, and writes:

- `session.mp4` for the user-facing recording.
- `frames/frame-*.png` for frame-by-frame agent inspection.
- `keyframes/frame-*.png` for action baseline and first-visible-change frames.
- `contact-sheet.png` for quick visual review.
- `metrics.json` for latency, blank-frame, and large-delta metrics.
- `inspection.txt` for a short checklist.

This is the preferred evidence for Kitty image protocol behavior because PTY
captures only prove that escape payload bytes were emitted. A real terminal
recording proves that the emulator composited the payload into the window.

Initial visual standards:

- The recording must have at least one nonblank frame after startup.
- For plot commands, the first nonblank frame must include colored series
  pixels, not only terminal chrome.
- After the first established draw, there should be no blank frames before the
  scripted quit action.
- At least one non-quit scripted action should have a detected first visible
  frame so the latency sample path is exercised. Inspect missing action samples
  manually; repeated or low-delta actions can fall below the frame-diff
  threshold even when the recording is visually healthy.
- Median visible latency should stay below roughly 150 ms on a warmed local
  run, and max visible latency should stay below roughly 300 ms. Treat these as
  investigation thresholds, not universal hardware-independent guarantees.
- Large full-window deltas should be inspected manually; they may indicate
  flicker, resize churn, or a legitimate large plot redraw.
- The first visible screen must still be inspected manually for missing image
  payloads or chart bodies; nonblank chrome alone is not enough to pass a Kitty
  visual regression check.

When reviewing the output, inspect `frames/` or `contact-sheet.png` yourself.
The MP4 is evidence for the user, but frame inspection and `metrics.json` are
the repeatable verification surface.

Required local tools for this path are `Xvfb`, `kitty`, `xdotool`, `ffmpeg`,
`xwininfo`, Python 3, and Pillow.

Use the fixture wrapper when a change can affect plot geometry, color, or input
shape handling:

```sh
scripts/record-emulator-fixtures.sh target/termviz-emulator-recordings/<name>
```

It records latency, throughput, error-spike, and scatter/outlier CSV fixtures
and writes a `summary.json` that points to each fixture's MP4, contact sheet,
and metrics file.

## PTY Recording

Use the PTY recording helper for block/TUI paths:

```sh
scripts/record-pty-demo.sh target/termviz-recordings/<name> -- target/debug/termviz examples/latency-demo.csv --x time --y latency --group service
```

The output directory is intentionally under `target/` so recordings are local
artifacts rather than source files. Each run writes:

- `session.gif` for sharing the final effect.
- `frames/frame-*.txt` and `frames/frame-*.ansi` for raw PTY frame inspection.
- `keyframes/frame-*.png` for agent-side visual inspection.
- `contact-sheet.png` for comparing representative frames at a glance.
- `manifest.json` for machine-readable metadata.
- `inspection.txt` for a short human-readable checklist.

For every TUI or block-rendering change, inspect the keyframes or contact sheet
before reporting completion. The goal is to catch blank screens, clipped status
lines, line drift, distracting backgrounds, illegible glyphs, and obvious
layout regressions. The contact sheet is generated from captured text frames,
so it is best for layout inspection. For status-bar color and background
changes, inspect `frames/*.ansi` or replay the raw ANSI capture in a terminal
that preserves styling.

For Kitty, ordinary PTY/tmux recording only captures escape payloads, not the
terminal GPU composited image. Pair PTY runs with `record-emulator-demo.sh` for
final visual verification. For calculatable plots, decode at least one Kitty
payload and inspect the embedded PNG: it should use the dark viewer theme and
an interactive raster size consistent with the current budget and not the
fixed-size export image. Interactive pixel-protocol plots intentionally keep
file path, legend, axis labels, and controls as terminal text outside the image
payload, so a PTY contact sheet should show the chrome even though it cannot
composite the image body. Also assert that sized protocols request the active
terminal cell area.

Recordings are also product demos. When a visual behavior changes meaningfully,
keep the latest local recording path in the handoff summary or PR notes so
reviewers can replay the current effect.
