use crossterm::style::Color;

#[derive(Debug, Clone)]
pub(crate) struct PlotProtocolFrame<'a> {
    pub(crate) payload: &'a str,
    pub(crate) body: PlotImageBody,
    pub(crate) chrome: PlotProtocolChrome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlotImageBody {
    pub(crate) col: u16,
    pub(crate) row: u16,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
}

#[derive(Debug, Clone)]
pub(crate) struct PlotProtocolChrome {
    pub(crate) static_layer: StaticPlotChrome,
    pub(crate) dynamic_layer: DynamicPlotChrome,
}

#[derive(Debug, Clone)]
pub(crate) struct StaticPlotChrome {
    pub(crate) repaint: bool,
    pub(crate) x_axis_row: u16,
    pub(crate) legend: Vec<PlotLegendItem>,
}

#[derive(Debug, Clone)]
pub(crate) struct DynamicPlotChrome {
    pub(crate) header: ChromeLine,
    pub(crate) y_labels: Vec<PlotAxisLabel>,
    pub(crate) x_labels: Vec<PlotAxisLabel>,
    pub(crate) status: ChromeLine,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ChromeLine {
    pub(crate) segments: Vec<ChromeSegment>,
}

impl ChromeLine {
    pub(crate) fn new(segments: impl Into<Vec<ChromeSegment>>) -> Self {
        Self {
            segments: segments.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChromeSegment {
    pub(crate) text: String,
    pub(crate) role: ChromeRole,
}

impl ChromeSegment {
    pub(crate) fn new(text: impl Into<String>, role: ChromeRole) -> Self {
        Self {
            text: text.into(),
            role,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChromeRole {
    Title,
    Meta,
    State,
    Action,
    Muted,
}

#[derive(Debug, Clone)]
pub(crate) struct PlotLegendItem {
    pub(crate) marker: String,
    pub(crate) label: String,
    pub(crate) color: Color,
}

#[derive(Debug, Clone)]
pub(crate) struct PlotAxisLabel {
    pub(crate) col: u16,
    pub(crate) row: u16,
    pub(crate) text: String,
}
