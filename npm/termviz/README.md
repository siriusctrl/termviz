# termviz

Terminal-first viewing for images and plots.

This npm package installs the prebuilt static Linux x64 `termviz` binary.

```sh
npm install -g termviz
termviz image.png
termviz chart.svg --inspect
termviz data.csv --x time --y latency --group service
```

`termviz` opens supported visual files in an interactive terminal viewer when
stdout is a TTY. Redirected stdout stays scriptable and defaults to PNG export,
with explicit JSON, ANSI, PNG, and SVG export formats available through
`--output-format`.

The npm package is a binary distribution wrapper. For source, full docs, Cargo
installation, and release notes, see:

https://github.com/siriusctrl/termviz
