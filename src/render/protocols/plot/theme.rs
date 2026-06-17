use image::Rgba;

pub(super) const EXPORT_BACKGROUND: Rgba<u8> = Rgba([255, 255, 255, 255]);

const EXPORT_CHART_STROKE: [[u8; 4]; 6] = [
    [31, 119, 180, 255],
    [255, 127, 14, 255],
    [44, 160, 44, 255],
    [214, 39, 40, 255],
    [148, 103, 189, 255],
    [140, 86, 75, 255],
];

const INTERACTIVE_CHART_STROKE: [[u8; 4]; 6] = [
    [45, 212, 191, 255],
    [245, 158, 11, 255],
    [129, 140, 248, 255],
    [244, 63, 94, 255],
    [56, 189, 248, 255],
    [163, 230, 53, 255],
];

#[derive(Debug, Clone, Copy)]
pub(super) struct PlotTheme {
    pub(super) background: Rgba<u8>,
    pub(super) axis: Rgba<u8>,
    pub(super) grid: Rgba<u8>,
    pub(super) text: Rgba<u8>,
    pub(super) title: Rgba<u8>,
    pub(super) strokes: &'static [[u8; 4]; 6],
    pub(super) series_width: i32,
    pub(super) mark_radius: i32,
}

pub(super) const EXPORT_THEME: PlotTheme = PlotTheme {
    background: EXPORT_BACKGROUND,
    axis: Rgba([42, 42, 42, 255]),
    grid: Rgba([228, 228, 228, 255]),
    text: Rgba([24, 24, 24, 255]),
    title: Rgba([40, 40, 40, 255]),
    strokes: &EXPORT_CHART_STROKE,
    series_width: 1,
    mark_radius: 2,
};

pub(super) const INTERACTIVE_THEME: PlotTheme = PlotTheme {
    background: Rgba([8, 12, 16, 255]),
    axis: Rgba([116, 140, 150, 255]),
    grid: Rgba([24, 36, 42, 255]),
    text: Rgba([202, 211, 216, 255]),
    title: Rgba([231, 238, 242, 255]),
    strokes: &INTERACTIVE_CHART_STROKE,
    series_width: 2,
    mark_radius: 3,
};
