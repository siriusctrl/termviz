use super::Protocol;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalCapabilities {
    pub(crate) preferred: Protocol,
}

pub(crate) fn detect(protocol: Protocol) -> TerminalCapabilities {
    if protocol == Protocol::Auto {
        return TerminalCapabilities {
            preferred: detect_auto_protocol(),
        };
    }

    TerminalCapabilities {
        preferred: protocol,
    }
}

fn detect_auto_protocol() -> Protocol {
    if std::env::var_os("KITTY_WINDOW_ID").is_some() {
        return Protocol::Kitty;
    }
    if std::env::var_os("ITERM_SESSION_ID").is_some() {
        return Protocol::Iterm;
    }

    Protocol::Blocks
}
