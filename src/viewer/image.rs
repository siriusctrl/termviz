use anyhow::{Result, bail};

use crate::{input::InputSource, profile::InputProfile, render::Protocol};

pub(crate) fn run(_source: InputSource, _profile: InputProfile, _protocol: Protocol) -> Result<()> {
    bail!(
        "interactive image viewer is not implemented yet; use --inspect for scaffolded profile output"
    )
}
