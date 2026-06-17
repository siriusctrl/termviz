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
    [86, 199, 217, 255],
    [242, 166, 90, 255],
    [136, 199, 121, 255],
    [216, 140, 190, 255],
    [143, 167, 255, 255],
    [233, 214, 107, 255],
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
    background: Rgba([9, 13, 15, 255]),
    axis: Rgba([111, 133, 136, 255]),
    grid: Rgba([27, 37, 40, 255]),
    text: Rgba([205, 212, 209, 255]),
    title: Rgba([238, 243, 234, 255]),
    strokes: &INTERACTIVE_CHART_STROKE,
    series_width: 2,
    mark_radius: 3,
};
