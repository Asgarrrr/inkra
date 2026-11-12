//! The last vault opened, reopened by a launch with no argument (open-a-vault spec, D4).

use std::path::{Path, PathBuf};

use inkra_vault::Fs;
use serde::{Deserialize, Serialize};

use crate::app_files::last_vault_file;

const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct LastVault {
    version: u32,
    root: PathBuf,
}

/// `None`, silently, when the file is missing, corrupt or of another version.
pub(crate) async fn read(fs: &dyn Fs, home: &Path) -> Option<PathBuf> {
    let loaded = fs.load(&last_vault_file(home)).await.ok()?;
    let file = serde_json::from_str::<LastVault>(&loaded.text).ok()?;
    (file.version == VERSION).then_some(file.root)
}

/// A failure is logged, never shown: the vault is open either way.
pub(crate) async fn write(fs: &dyn Fs, home: &Path, root: &Path) {
    let file = last_vault_file(home);
    let last = LastVault {
        version: VERSION,
        root: root.to_owned(),
    };
    let Ok(bytes) = serde_json::to_vec(&last)
        .inspect_err(|error| log::warn!("last vault not saved: {}: {error}", file.display()))
    else {
        return;
    };
    if let Err(error) = fs.replace_bytes(&file, &bytes).await {
        log::warn!("last vault not saved: {error}");
    }
}

#[cfg(test)]
mod tests;
