pub(crate) mod image;
pub(crate) mod plot;

use anyhow::{Result, bail};

use crate::{
    input::InputSource,
    plot::PlotKind,
    profile::{InputProfile, RenderStrategy},
    render,
    render::Protocol,
};

pub(crate) struct ViewerRequest {
    pub(crate) protocol: Protocol,
    pub(crate) x: Option<String>,
    pub(crate) y: Option<String>,
    pub(crate) group: Option<String>,
    pub(crate) kind: PlotKind,
}

pub(crate) fn validate_plot_request(request: &ViewerRequest, profile: &InputProfile) -> Result<()> {
    if profile.render != RenderStrategy::TerminalPlot {
        return Ok(());
    }

    if request.x.is_none() || request.y.is_none() {
        bail!("interactive plot viewer requires --x and --y");
    }
    Ok(())
}

pub(crate) fn resolve_protocol(protocol: Protocol) -> Protocol {
    match protocol {
        Protocol::Auto | Protocol::Kitty | Protocol::Sixel | Protocol::Iterm | Protocol::Blocks => {
            protocol
        }
    }
}

pub(crate) fn run(
    source: InputSource,
    profile: InputProfile,
    request: ViewerRequest,
) -> Result<()> {
    validate_plot_request(&request, &profile)?;
    let protocol = resolve_viewer_protocol(&profile, request.protocol);

    match profile.render {
        RenderStrategy::TerminalImage => image::run(source, profile, protocol),
        RenderStrategy::TerminalPlot => plot::run(
            source,
            profile,
            protocol,
            plot::PlotRequest {
                x: request.x,
                y: request.y,
                group: request.group,
                kind: request.kind,
            },
        ),
    }
}

fn resolve_viewer_protocol(profile: &InputProfile, requested: Protocol) -> Protocol {
    match profile.render {
        RenderStrategy::TerminalImage | RenderStrategy::TerminalPlot => {
            render::terminal::detect(resolve_protocol(requested)).preferred
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use crate::{input::InputSource, plot::PlotKind, profile::InputProfile, render::Protocol};

    use super::*;

    #[test]
    fn validates_plot_request_axes_before_tui_entry() {
        let mut file = NamedTempFile::with_suffix(".csv").unwrap();
        writeln!(file, "time,latency").unwrap();
        writeln!(file, "1,20").unwrap();
        let source = InputSource::from_path(file.path().to_path_buf()).unwrap();
        let profile = InputProfile::resolve(&source, PlotKind::Line, None).unwrap();

        let request = ViewerRequest {
            protocol: Protocol::Blocks,
            x: Some("time".to_owned()),
            y: None,
            group: None,
            kind: PlotKind::Line,
        };

        let err = validate_plot_request(&request, &profile).unwrap_err();
        assert!(err.to_string().contains("requires --x and --y"));
    }

    #[test]
    fn resolves_requested_protocol_for_tty_viewers() {
        let kitten_protocol = super::resolve_protocol(Protocol::Kitty);
        assert_eq!(kitten_protocol, Protocol::Kitty);

        let auto_protocol = super::resolve_protocol(Protocol::Auto);
        assert_eq!(auto_protocol, Protocol::Auto);
    }

    #[test]
    fn plot_viewer_respects_requested_image_protocols() {
        let mut file = NamedTempFile::with_suffix(".csv").unwrap();
        writeln!(file, "time,latency").unwrap();
        writeln!(file, "1,20").unwrap();
        let source = InputSource::from_path(file.path().to_path_buf()).unwrap();
        let profile = InputProfile::resolve(&source, PlotKind::Line, None).unwrap();

        let protocol = super::resolve_viewer_protocol(&profile, Protocol::Kitty);

        assert_eq!(protocol, Protocol::Kitty);
    }
}
