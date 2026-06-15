pub(crate) mod model;
pub(crate) mod stream;
pub(crate) mod table;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlotKind {
    Line,
    Scatter,
}
