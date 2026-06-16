# Visual Verification

Terminal UI changes should leave visual evidence, not just command output.

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
layout regressions.

For pixel protocols such as Kitty and iTerm2, ordinary PTY/tmux recording only
captures escape payloads, not the terminal GPU composited image. Pair those
runs with protocol-payload checks and, when available, a real terminal
screenshot or screen recording.

Recordings are also product demos. When a visual behavior changes meaningfully,
keep the latest local recording path in the handoff summary or PR notes so
reviewers can replay the current effect.
