pub(crate) mod image;
pub(crate) mod plot;

use anyhow::Result;

use crate::{input::InputSource, profile::InputProfile, render};

pub(crate) fn run(
    source: InputSource,
    profile: InputProfile,
    protocol: render::Protocol,
) -> Result<()> {
    let capabilities = render::terminal::detect(protocol);
    match profile.render {
        crate::profile::RenderStrategy::TerminalImage => {
            image::run(source, profile, capabilities.preferred)
        }
        crate::profile::RenderStrategy::TerminalPlot => plot::run(source, profile),
    }
}
