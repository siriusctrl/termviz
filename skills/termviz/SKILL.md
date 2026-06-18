---
name: termviz
description: Use when installing, launching, scripting, or explaining termviz, a terminal-first image and plot viewer with interactive TTY rendering, scriptable exports, and CSV/TSV/JSONL plot support.
metadata:
  internal: false
---

# termviz

Use `termviz` when a user wants to inspect, view, or export images and small
numeric plots from a shell. It is a local terminal viewer, not a server,
dashboard, notebook runtime, or background daemon.

## Install

Install from GitHub with Cargo:

```sh
cargo install --git https://github.com/siriusctrl/termviz
```

Install a specific release tag when reproducibility matters:

```sh
cargo install --git https://github.com/siriusctrl/termviz --tag vX.Y.Z
```

The install provides both binaries:

```sh
termviz --version
tvz --version
```

Use `termviz` in examples when clarity matters. Use `tvz` as the short alias
for daily interactive use.

## Core Behavior

- If stdout is a TTY, opening a supported input starts the interactive viewer.
- If stdout is redirected, the default output is scriptable PNG bytes, not
  terminal escape sequences.
- Use `--inspect` for metadata/profile text.
- Use `--output-format` or `--output` for explicit exports.
- Use `--protocol auto` by default. Force `kitty` or `blocks` only when testing
  or overriding terminal detection.

## Interactive Use

Open image inputs:

```sh
termviz image.png
tvz image.webp
termviz photo.jpg --protocol auto
```

Open plot inputs:

```sh
termviz data.csv --x time --y latency
termviz data.csv --x time --y latency --group service
termviz data.csv --x load_ms --y cpu_pct --kind scatter --group node
```

Common controls:

- `q`: quit
- `+` / `-`: zoom in and out
- `0`: fit to terminal
- arrow keys: pan
- `m`: toggle metadata or plot summary overlay
- mouse hover on plots: snap to nearest visible point and show x/y readout
- left mouse drag on images: pan image inputs

## Plot Arguments

Use this argument shape for numeric table or stream data:

```sh
termviz INPUT --x X_FIELD --y Y_FIELD --group GROUP_FIELD --kind KIND
```

Supported input formats:

- Raster images: PNG, JPEG, WebP, GIF metadata/static viewing path
- Vector metadata/export: SVG
- Plot data: CSV, TSV, JSONL/NDJSON

Supported plot kinds:

- `line`: numeric `--x` and `--y`, optional `--group`
- `scatter`: numeric `--x` and `--y`, optional `--group`
- `bar`: numeric `--x` and `--y`, optional `--group`
- `area`: numeric `--x` and `--y`, optional `--group`
- `histogram`: numeric `--x`, optional `--group`, no `--y`

Current bar and histogram support is numeric-axis first. Do not describe them
as categorical charts unless the code has been extended.

## Export Patterns

Redirected stdout defaults to PNG:

```sh
termviz image.png > frame.png
termviz data.csv --x time --y latency > chart.png
```

Choose an explicit output format:

```sh
termviz image.png --output-format json > metadata.json
termviz image.png --output-format ansi > preview.ansi
termviz data.csv --x time --y latency --output-format svg > chart.svg
termviz data.csv --x time --y latency --output-format json > chart.json
```

Let `termviz` infer export format from `--output`:

```sh
termviz image.png --output frame.png
termviz data.csv --x time --y latency --output chart.svg
termviz data.csv --x load_ms --kind histogram --output histogram.json
```

Shell redirection does not expose the target filename to `termviz`; use
`--output-format` when redirecting to non-PNG formats.

## Useful Recipes

Inspect a file without opening the viewer:

```sh
termviz input.data --input-format csv --inspect
```

Compare services over time:

```sh
termviz metrics.csv --x minute --y latency --group service --kind line
```

Show grouped bars:

```sh
termviz errors.csv --x minute --y errors --group service --kind bar
```

Show an area trend:

```sh
termviz throughput.csv --x minute --y throughput --group region --kind area
```

Show a grouped distribution:

```sh
termviz samples.jsonl --input-format jsonl --x latency_ms --group endpoint --kind histogram
```

## Guardrails For Agents

- Keep stdout scriptable. Never rely on implicit protocol escape output in
  redirected stdout.
- Prefer `--inspect`, `--output-format json`, or explicit file exports in
  automation.
- For interactive demos or visual verification, use a real TTY or the repo's
  recording scripts instead of plain redirected command output.
- For large raster inputs, expect interactive viewing to be guarded until
  tile-backed rendering exists.
