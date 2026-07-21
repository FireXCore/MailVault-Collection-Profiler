use std::path::{Path, PathBuf};

use profiler_core::{ProfilerError, ProfilerResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailVaultLayout {
    pub root: PathBuf,
    pub database: PathBuf,
    pub raw_objects: PathBuf,
    pub blob_objects: PathBuf,
    pub state: PathBuf,
    pub sync_lock: PathBuf,
}

impl MailVaultLayout {
    pub fn inspect(root: &Path) -> ProfilerResult<Self> {
        let root = std::fs::canonicalize(root).map_err(|source| ProfilerError::Io {
            operation: "canonicalizing MailVault archive root",
            path: root.to_path_buf(),
            source,
        })?;
        if !root.is_dir() {
            return Err(ProfilerError::InvalidPath {
                message: "MailVault archive root is not a directory".into(),
                path: root,
            });
        }

        Ok(Self {
            database: root.join("database").join("mailvault.sqlite3"),
            raw_objects: root.join("objects").join("raw").join("sha256"),
            blob_objects: root.join("objects").join("blobs").join("sha256"),
            state: root.join("state"),
            sync_lock: root.join("state").join("sync.lock"),
            root,
        })
    }
}
