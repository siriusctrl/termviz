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

Plots currently implement the calculatable scene path. Kitty renders the
current plot viewport by recalculating the chart for the active terminal shape
and dark viewer theme. Plot raster and SVG export share a small
internal display list that owns layout, visible-range clipping, axis labels,
legend commands, and dense-line downsampling. Export PNG rasterizes that full
display list; SVG export writes it as vector elements. Interactive pixel
protocols use a different split: file path, legend, and axis labels are real
terminal text around the image, with a compact control-only bottom bar. The
image protocol carries only the chart body marks, grid, frame, and dark matte.
Blocks renders a dark terminal-native
Braille fallback because terminal-cell output has different constraints. SVG
input files are profiled as future calculatable scenes, but interactive SVG
rasterization is still gated until an SVG rasterizer is added.

Interactive plot viewing keeps terminal input ahead of expensive protocol
payload work. The event loop drains pending key and resize events before drawing
so burst input renders the latest state instead of every intermediate state. It
keeps a bounded cache of encoded frames by protocol, plot kind, viewport, and
terminal size. After user navigation, a small background prefetcher warms likely
next frames without blocking the foreground draw. For repeated pan actions on
large scenes, the prefetcher can render a transparent marks atlas once and crop
future same-zoom pan frames, then composite those marks over the current
grid/frame layer so axis labels and grid lines stay correct.

Kitty plot frames use zlib-compressed raw RGBA direct-data payloads so terminal
updates avoid PNG decode work while still working when the terminal process
cannot read files from the app's filesystem, such as SSH, container, or
sandboxed sessions. Prefetched Kitty frames are transmitted with image IDs
during idle time. If the next key lands on an already-transmitted frame, the
foreground path writes only a small image placement command instead of sending
the image bytes again. Each visible plot image uses a stable placement id and a
unique image id. The currently visible image is never selected for idle
pretransmit work; if a visible frame has not been transmitted yet, the
foreground draw sends its display payload and marks it transmitted there.
Updates place the new image first, then delete only the previous visible image
placement by image id. All visible plot placements share the same stable
z-index, avoiding broad z-index or full-screen deletes that can blank the plot
during fast navigation. The prefetch list stays intentionally small: more
candidates increase background raster work and hidden terminal bytes, so newer
directional batches suppress stale, not-yet-transmitted candidates.

Kitty frames request the full terminal cell area while rendering normal terminal
windows at the full terminal pixel estimate. Very large windows use a bounded
internal raster budget to keep redraw and protocol encoding cost predictable.
The terminal chrome is rendered as a styled status bar with a stable dark
background and segmented state text. For plot pixel protocols, the chrome also
owns the header, legend, and axis labels so crisp terminal text surrounds a
smaller body-only image payload; static chrome is repainted only on first draw
or resize. On full chrome repaints, the terminal chrome is drawn before the
Kitty image payload is written. That ordering lets real terminal recordings see
the chart body in the first composited frame instead of a chrome-only screen.

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
should fail with a clear message pointing to `--input-format` instead of
guessing a plot.

## Terminal Render Backends

Terminal image protocols are render backends, not separate products:

```text
  render::terminal
    capability detection and protocol selection

  render::protocols::kitty
    Kitty graphics protocol

  render::protocols::blocks
    portable ANSI truecolor block fallback
```

The CLI exposes `--protocol auto|kitty|blocks` for interactive raster viewing
and calculatable plot scenes. `auto` is the default path and prefers known
Kitty-compatible terminal hints from Kitty, WezTerm, and Ghostty. If no reliable
Kitty capability is visible in the environment, it falls back to blocks.
Terminal multiplexers such as tmux and screen fall back to blocks only when no
known outer-terminal hint is visible; if Ghostty, WezTerm, or Kitty leaves a
clear environment hint, `auto` still selects the Kitty-compatible path. Explicit
protocol flags should stay deterministic and testable.

Interactive raster fit mode builds a terminal-shaped dark RGBA canvas before
protocol output. The source image is scaled proportionally, centered on that
canvas, and alpha-composited against the dark matte. Kitty payloads request the
active terminal cell dimensions for fitted interactive frames. Blocks renders a
cell-sized fallback canvas.

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

  viewer/plot/
    interactive plot loop, state changes, event handling, frame caching, and
    terminal chrome assembly

  tui/
    terminal session orchestration, palette, layout, styled chrome, dimensions,
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
  PlotDisplayList
    layout, text, clipped marks, dense-line buckets
          |
          +--> PNG export raster frame
          +--> body-only image protocol frame + terminal chrome
          +--> SVG export
          +--> terminal cell fallback
          |
          v
  render backend
```

The first plot milestone supports line and scatter plots from CSV, TSV, and
JSONL, with bounded loading capped at 1024 rows or records. Interactive plot
scenes prefer image protocols for smooth terminal rendering and rasterize at the
current terminal shape rather than scaling a fixed export image. The plot
display list is deliberately private to `render::protocols::plot`; it is an
implementation boundary for sharing layout and clipping between PNG/SVG/image
protocol renderers, not a public plotting API. Kitty renders normal terminal
sizes at the full terminal pixel estimate; only very large windows may use a
smaller internal raster and ask the terminal protocol to place that image across
the active cell area. Blocks stays a Braille fallback for terminals without
image protocol support. Additional chart types are useful only after the
data-window, axis, and render boundaries are stable.

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
- PNG export output by default when stdout is redirected;
- explicit non-PNG export output when `--output-format` is used with shell
  redirection, or when `--output` has a supported extension;
- a clear error when the requested export is not supported for that input.

This rule keeps shell composition predictable and makes protocol output a
deliberate user choice.
