use anyhow::{Result, bail};

use crate::{input::InputSource, profile::InputProfile};

pub(crate) fn run(_source: InputSource, _profile: InputProfile) -> Result<()> {
    bail!(
        "interactive plot viewer is not implemented yet; use --inspect for scaffolded profile output"
    )
}
