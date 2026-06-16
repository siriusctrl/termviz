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
    detect_auto_protocol_from_env_hints(|key| std::env::var(key).ok())
}

fn detect_auto_protocol_from_env_hints(
    mut get_env: impl FnMut(&str) -> Option<String>,
) -> Protocol {
    let term = get_env("TERM").unwrap_or_default();
    let term_program = get_env("TERM_PROGRAM").unwrap_or_default();
    let lc_terminal = get_env("LC_TERMINAL").unwrap_or_default();

    if get_env("KITTY_WINDOW_ID").is_some()
        || term.eq_ignore_ascii_case("xterm-kitty")
        || env_value_matches(&term_program, "kitty")
    {
        return Protocol::Kitty;
    }

    if get_env("WEZTERM_PANE").is_some()
        || get_env("WEZTERM_EXECUTABLE").is_some()
        || env_value_matches(&term_program, "wezterm")
        || env_value_matches(&lc_terminal, "wezterm")
        || get_env("GHOSTTY_RESOURCES_DIR").is_some()
        || env_value_matches(&term_program, "ghostty")
        || env_value_matches(&term, "ghostty")
    {
        return Protocol::Kitty;
    }

    if get_env("TMUX").is_some()
        || get_env("STY").is_some()
        || env_value_matches(&term, "screen")
        || env_value_matches(&term, "tmux")
    {
        return Protocol::Blocks;
    }

    Protocol::Blocks
}

fn env_value_matches(value: &str, needle: &str) -> bool {
    value.to_ascii_lowercase().contains(needle)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn detect_from(pairs: &[(&str, &str)]) -> Protocol {
        let env = pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect::<HashMap<_, _>>();
        detect_auto_protocol_from_env_hints(|key| env.get(key).cloned())
    }

    #[test]
    fn auto_prefers_known_pixel_protocol_environment_hints() {
        assert_eq!(detect_from(&[("KITTY_WINDOW_ID", "1")]), Protocol::Kitty);
        assert_eq!(detect_from(&[("TERM_PROGRAM", "WezTerm")]), Protocol::Kitty);
        assert_eq!(detect_from(&[("TERM", "xterm-ghostty")]), Protocol::Kitty);
        assert_eq!(
            detect_from(&[("GHOSTTY_RESOURCES_DIR", "/opt/ghostty")]),
            Protocol::Kitty
        );
    }

    #[test]
    fn auto_falls_back_to_blocks_without_protocol_hints() {
        assert_eq!(detect_from(&[("TERM", "xterm-256color")]), Protocol::Blocks);
        assert_eq!(detect_from(&[]), Protocol::Blocks);
    }

    #[test]
    fn auto_prefers_outer_terminal_hints_inside_multiplexers() {
        assert_eq!(
            detect_from(&[("KITTY_WINDOW_ID", "1"), ("TMUX", "/tmp/tmux")]),
            Protocol::Kitty
        );
        assert_eq!(
            detect_from(&[("TERM_PROGRAM", "WezTerm"), ("TERM", "screen-256color")]),
            Protocol::Kitty
        );
        assert_eq!(
            detect_from(&[
                ("TERM", "screen-256color"),
                ("TERM_PROGRAM", "ghostty"),
                ("TMUX", "/tmp/tmux")
            ]),
            Protocol::Kitty
        );
    }

    #[test]
    fn auto_falls_back_inside_multiplexers_without_outer_terminal_hints() {
        assert_eq!(
            detect_from(&[("TMUX", "/tmp/tmux"), ("TERM", "screen-256color")]),
            Protocol::Blocks
        );
        assert_eq!(
            detect_from(&[("STY", "1234.pts"), ("TERM", "screen-256color")]),
            Protocol::Blocks
        );
    }
}
