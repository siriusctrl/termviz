#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlotScene {
    pub(crate) title: Option<String>,
    pub(crate) series: Vec<PlotSeries>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlotSeries {
    pub(crate) name: String,
    pub(crate) points: Vec<PlotPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlotPoint {
    pub(crate) x: f64,
    pub(crate) y: f64,
}
