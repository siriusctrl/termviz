use std::{
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

pub(crate) mod sniff;

#[derive(Debug, Clone)]
pub(crate) struct InputSource {
    path: PathBuf,
    label: String,
}

impl InputSource {
    pub(crate) fn from_path(path: PathBuf) -> Result<Self> {
        let label = path.display().to_string();
        let metadata = path
            .metadata()
            .with_context(|| format!("failed to read metadata for {label}"))?;
        if !metadata.is_file() {
            anyhow::bail!("{label} is not a file");
        }
        Ok(Self { path, label })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn open(&self) -> Result<File> {
        File::open(&self.path).with_context(|| format!("failed to open {}", self.label))
    }
}
