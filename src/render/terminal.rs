use super::Protocol;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalCapabilities {
    pub(crate) preferred: Protocol,
}

pub(crate) fn detect(protocol: Protocol) -> TerminalCapabilities {
    TerminalCapabilities {
        preferred: match protocol {
            Protocol::Auto => Protocol::Blocks,
            explicit => explicit,
        },
    }
}
