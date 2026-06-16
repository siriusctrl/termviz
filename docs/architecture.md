# Architecture

`termviz` is a terminal viewer first. Rendering image protocols and plotting
data are preparation steps for the viewer, redirected output, and explicit
exports, but the product surface is broader: inspect metadata, pan, zoom, and
export visual inputs without leaving the CLI.

The core boundary is:

```text
  +------------------+      +------------------+      +-------------------+
  | Use case         |      | Input type       |      | InputProfile      |
  |                  |      |                  |      |                   |
  | InteractiveView  +----->+ png/jpeg/webp    +----->+ content kind      |
  | RedirectedInspect|      | csv/jsonl        |      | content shape     |
  | ExplicitExport   |      | svg/plot spec    |      | load strategy     |
  +------------------+      +------------------+      | render strategy   |
                                                      | export policy     |
                                                      +---------+---------+
                                                                |
                                                                v
                       +----------------+-----------------------+-------------+
                       |                |                       |             |
                       v                v                       v             v
                 +-----------+    +-------------+        +------------+  +--------+
                 | Asset load|    | Plot model  |        | TTY render |  | Export |
                 +-----------+    +-------------+        +------------+  +--------+
```

`InputProfile` selects the shared runtime behavior for the current input. It
answers five questions:

- What is the content kind?
- What content shape does this type expose to the viewer pipeline?
- Should loading be metadata-only, tiled, streamed, generated, or eager?
- Which terminal render strategy should be tried first?
- What is safe to do when stdout is redirected?

## Content Shapes

`ContentShape` is the coarse performance and capability boundary. It is not a
decoder interface and it does not decide terminal protocol output by itself.
It names the unit of work that shared runtimes can rely on:

```text
  RasterImage
    A single still image with pixel dimensions. PNG, JPEG, WebP, and static GIF
    use this shape. The first draw should prefer metadata-first loading and
    then decode only what the current viewport needs when tiled decoding exists.

  VectorImage
    A scalable visual document such as SVG. The first implementation may
    rasterize eagerly for small files, but large or complex rasterization must
    be explicit in docs.

  AnimatedFrames
    A sequence of image frames. GIF and future animated WebP use this shape.
    The viewer should decode frame metadata first and render bounded frame
    windows.

  DataTable
    Row/column data that can become a plot. CSV and TSV use this shape. The
    first implementation should sniff headers and bounded sample rows before
    building a complete model.

  DataStream
    Newline-delimited records that can become a plot. JSONL uses this shape.
    Plot windows and grouping should stay incremental where possible.

  PlotSpec
    A declarative plot specification such as future Vega-Lite support. This is
    not an initial milestone because it can pull the product toward a plotting
    runtime instead of a terminal viewer.
```

Optimizations should say which shape they target. Tile work belongs to raster
assets. Data-window work belongs to table or stream plot inputs. Terminal
protocol output should stay below the render boundary.

## Visual Source Classes

Interactive viewing splits inputs into two display classes after profiling:

```text
  RasterSource
    PNG/JPEG/WebP/GIF frames that already exist as pixels. The viewer maps the
    current image viewport to a terminal render backend.

  CalculatableScene
    Plots, SVGs, and future visual specs that can be recalculated for a target
    viewport, size, and theme before rendering. The preferred interactive path
    is scene -> RGBA image -> terminal image protocol. The blocks backend is a
    portable cell fallback, not the quality target.
```

Plots currently implement the calculatable scene path. Kitty, Sixel, and iTerm2
render the current plot viewport through the raster chart pipeline used by PNG
export; blocks renders a dark terminal-native Braille fallback. SVG is profiled
as a future calculatable scene, but interactive SVG rasterization is still gated
until an SVG rasterizer is added.

## Current Profiles

| Type | Shape | Interactive view | Redirected stdout | Export | Package |
| --- | --- | --- | --- | --- | --- |
| PNG/JPEG/WebP | RasterImage | image viewer | inspect only | explicit JSON/ANSI/PNG export | `asset::raster` |
| GIF | AnimatedFrames | metadata/profile support | inspect only | explicit metadata output | `asset::raster` |
| SVG | VectorImage | metadata/profile support | inspect only | explicit JSON/SVG export | `asset::svg` |
| CSV/TSV | DataTable | plot viewer with `--x`/`--y` | inspect only | explicit JSON/ANSI/SVG plot export | `plot::table` |
| JSONL/NDJSON | DataStream | plot viewer with `--x`/`--y` | inspect only | explicit JSON/ANSI/SVG plot export | `plot::stream` |
| Vega/Vega-Lite | PlotSpec | future plot viewer | inspect only | explicit plot export | future |

Unknown extensions should be sniffed with a bounded prefix. Unknown content
should fail with a clear message instead of guessing a plot.

## Terminal Render Backends

Terminal image protocols are render backends, not separate products:

```text
  render::terminal
    capability detection and protocol selection

  render::protocols::kitty
    Kitty graphics protocol

  render::protocols::sixel
    Sixel image output

  render::protocols::iterm
    iTerm2 inline images

  render::protocols::blocks
    portable ANSI truecolor block fallback
```

The CLI exposes `--protocol auto|kitty|sixel|iterm|blocks` for interactive
raster viewing and calculatable plot scenes. `auto` is the default path and
prefers known pixel protocol hints first: Kitty-compatible terminals such as
Kitty, WezTerm, and Ghostty, then iTerm2, then explicit Sixel terminal hints. If
no reliable pixel capability is visible in the environment, it falls back to
blocks. Terminal multiplexers such as tmux and screen also fall back to blocks
by default because passthrough support is configuration-dependent. Explicit
protocol flags should stay deterministic and testable.

Interactive raster fit mode builds a terminal-shaped dark RGBA canvas before
protocol output. The source image is scaled proportionally, centered on that
canvas, and alpha-composited against the dark matte. Kitty and iTerm payloads
request the active terminal cell dimensions for fitted interactive frames. Sixel
does not expose the same cell-placement control, so the renderer scales toward
a conservative terminal-pixel estimate before encoding.

Protocol output must never appear on redirected stdout unless the user chooses
an explicit render/export path.

## Viewer Lifecycle

The shared viewer layer owns raw mode, alternate screen, cleanup, mouse capture,
and dispatch:

```text
  viewer.rs
    TTY lifecycle and mode dispatch

  viewer/image.rs
    keyboard and mouse pan, zoom, fit, actual-ish size, metadata overlay, and
    future frame navigation

  viewer/plot.rs
    plot render, resize redraw, summary overlay, future pan/zoom,
    future series/legend navigation, future point inspection

  tui/
    reusable terminal primitives: palette, layout, dimensions, text overlays,
    protocol placement, and future buffer-delta repainting
```

The normal image viewer and plot viewer should share terminal lifecycle and
render placement primitives. Their models stay separate because image viewing
is viewport over visual pixels, while plot viewing is viewport over data-space
and visual encodings.

## Plot Model

Do not make each data format draw directly to the terminal. Convert data inputs
into a small internal model first:

```text
  CSV/JSONL sample or window
          |
          v
  typed columns and series
          |
          v
  PlotScene
    axes, series, legend, viewport, marks
          |
          v
  raster image protocol or terminal cell fallback
          |
          v
  render backend
```

The first plot milestone supports line and scatter plots from CSV, TSV, and
JSONL, with bounded loading capped at 1024 rows or records. Plot scenes prefer
image protocols for smooth terminal rendering and keep blocks as a Braille
fallback for terminals without image protocol support. Additional chart types
are useful only after the data-window, axis, and render boundaries are stable.

## Known Tradeoffs

Interactive image viewing currently reads raster dimensions before opening the
viewer, refuses inputs above the safety guard, and then decodes the full raster
before first draw. Tile-based image readback and preloading are future work and
should be called out in user-facing docs until they exist.

Explicit raster export still decodes the full image before writing PNG or ANSI
output.

Plot inputs are bounded to the first 1024 rows or records for the current
release. That keeps first-release behavior predictable, but it is not yet a
general streaming plot window with pan-ahead preloading.

## Export Policy

`termviz input > file` should not produce terminal image escape sequences by
default. Redirected stdout should be one of:

- inspect output when `--inspect` or future `--json` is used;
- explicit export output when `--format` or `--output` is used;
- a clear error explaining that interactive viewing requires a TTY.

This rule keeps shell composition predictable and makes protocol output a
deliberate user choice.
