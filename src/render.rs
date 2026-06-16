pub(crate) mod protocols;
pub(crate) mod terminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Protocol {
    Auto,
    Kitty,
    Blocks,
}
