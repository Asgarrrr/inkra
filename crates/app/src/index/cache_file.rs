//! The index cache file's read, off the foreground; its outcome is a turn input (persisted index spec, P7).

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;

use inkra_index::persist::{self, Outcome, decode};
use inkra_vault::Fs;

/// The cache of the vault at `root`: a missing file is silent, any other failure logs one line.
/// A rejected file stays on disk: the next write replaces it.
pub(super) async fn read(fs: Arc<dyn Fs>, file: PathBuf, root: PathBuf) -> Outcome {
    let bytes = match fs.read_bytes(&file, persist::MAX_BYTES).await {
        Ok(bytes) => bytes,
        Err(error) if error.io_kind() == Some(ErrorKind::NotFound) => return Outcome::Missing,
        Err(error) => {
            log::warn!("index cache not used: {}: {error}", file.display());
            return Outcome::Missing;
        }
    };
    match decode(&bytes, &root) {
        Ok(decoded) => Outcome::Decoded(decoded),
        Err(rejected) => {
            log::warn!(
                "index cache not used: {}: {}",
                file.display(),
                rejected.reason()
            );
            Outcome::Rejected(rejected)
        }
    }
}
