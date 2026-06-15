# Architecture

`termviz` is a terminal viewer first. Rendering image protocols and plotting
data are preparation steps for the viewer, redirected output, and explicit
exports, but the product surface is broader: inspect, pan, zoom, search
metadata, and export visual inputs without leaving the CLI.

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

## Current Profiles

| Type | Shape | Interactive view | Redirected stdout | Export | Package |
| --- | --- | --- | --- | --- | --- |
| PNG/JPEG/WebP | RasterImage | image viewer | inspect only | explicit image/ANSI export | `asset::raster` |
| GIF | AnimatedFrames | image viewer with frame control | inspect only | explicit frame export | `asset::raster` |
| SVG | VectorImage | rasterized image viewer | inspect only | explicit SVG/raster export | `asset::svg` |
| CSV/TSV | DataTable | plot viewer | inspect only | explicit plot export | `plot::table` |
| JSONL/NDJSON | DataStream | plot viewer | inspect only | explicit plot export | `plot::stream` |
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

The CLI should expose `--protocol auto|kitty|sixel|iterm|blocks`. `auto` may
inspect environment variables and terminal responses, but explicit protocol
flags should be deterministic and testable.

Protocol output must never appear on redirected stdout unless the user chooses
an explicit render/export path.

## Viewer Lifecycle

The shared viewer layer owns raw mode, alternate screen, mouse capture, cleanup,
and dispatch:

```text
  viewer.rs
    TTY lifecycle and mode dispatch

  viewer/image.rs
    pan, zoom, fit, actual size, frame navigation, metadata overlays

  viewer/plot.rs
    plot pan, zoom, series/legend navigation, point inspection

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
  raster or terminal scene
          |
          v
  render backend
```

The first plot milestone should support line and scatter plots. Additional
chart types are useful only after the data-window, axis, and render boundaries
are stable.

## Export Policy

`termviz input > file` should not produce terminal image escape sequences by
default. Redirected stdout should be one of:

- inspect output when `--inspect` or future `--json` is used;
- explicit export output when `--format` or `--output` is used;
- a clear error explaining that interactive viewing requires a TTY.

This rule keeps shell composition predictable and makes protocol output a
deliberate user choice.
