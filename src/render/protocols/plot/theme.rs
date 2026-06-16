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
    [96, 165, 250, 255],
    [251, 146, 60, 255],
    [52, 211, 153, 255],
    [248, 113, 113, 255],
    [196, 181, 253, 255],
    [244, 114, 182, 255],
];

#[derive(Debug, Clone, Copy)]
pub(super) struct PlotTheme {
    pub(super) background: Rgba<u8>,
    pub(super) axis: Rgba<u8>,
    pub(super) grid: Rgba<u8>,
    pub(super) text: Rgba<u8>,
    pub(super) title: Rgba<u8>,
    pub(super) strokes: &'static [[u8; 4]; 6],
}

pub(super) const EXPORT_THEME: PlotTheme = PlotTheme {
    background: EXPORT_BACKGROUND,
    axis: Rgba([42, 42, 42, 255]),
    grid: Rgba([228, 228, 228, 255]),
    text: Rgba([24, 24, 24, 255]),
    title: Rgba([40, 40, 40, 255]),
    strokes: &EXPORT_CHART_STROKE,
};

pub(super) const INTERACTIVE_THEME: PlotTheme = PlotTheme {
    background: Rgba([13, 17, 23, 255]),
    axis: Rgba([148, 163, 184, 255]),
    grid: Rgba([45, 55, 72, 255]),
    text: Rgba([203, 213, 225, 255]),
    title: Rgba([226, 232, 240, 255]),
    strokes: &INTERACTIVE_CHART_STROKE,
};
