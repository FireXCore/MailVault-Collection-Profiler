use std::{
    fs::{self, File, Metadata},
    io,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use profiler_core::{
    AvailabilityState, FileStatObservation, FileStatWorkItem, PhysicalObjectResolver, SizeState,
};

#[derive(Debug, Clone)]
pub struct MailVaultPhysicalObjectResolver {
    canonical_archive_root: PathBuf,
}

impl MailVaultPhysicalObjectResolver {
    pub fn new(archive_root: &Path) -> io::Result<Self> {
        Ok(Self {
            canonical_archive_root: fs::canonicalize(archive_root)?,
        })
    }
}

impl PhysicalObjectResolver for MailVaultPhysicalObjectResolver {
    fn inspect(&self, _archive_root: &Path, item: &FileStatWorkItem) -> FileStatObservation {
        if item.sha256.len() != 64 || !item.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return FileStatObservation::unavailable(
                item,
                String::new(),
                AvailabilityState::InvalidLocator,
                "invalid_sha256",
                "content object does not contain a valid SHA-256 value",
            );
        }
        let expected_locator = expected_blob_locator(&item.sha256);
        if item.source_locator != expected_locator {
            return FileStatObservation::unavailable(
                item,
                expected_locator,
                AvailabilityState::InvalidLocator,
                "locator_mismatch",
                "source blob locator does not match the canonical MailVault content-addressed path",
            );
        }

        let relative_path = Path::new(&item.source_locator);
        if let Err(message) = validate_relative_locator(relative_path) {
            return FileStatObservation::unavailable(
                item,
                expected_locator,
                AvailabilityState::InvalidLocator,
                "invalid_relative_locator",
                message,
            );
        }

        let candidate = self.canonical_archive_root.join(relative_path);
        if let Err(observation) = inspect_path_chain(
            &self.canonical_archive_root,
            relative_path,
            item,
            &expected_locator,
        ) {
            return *observation;
        }

        let canonical_candidate = match fs::canonicalize(&candidate) {
            Ok(path) => path,
            Err(error) => {
                return unavailable_from_io(item, expected_locator, "canonicalize", &error);
            }
        };
        if !canonical_candidate.starts_with(&self.canonical_archive_root) {
            return FileStatObservation::unavailable(
                item,
                expected_locator,
                AvailabilityState::InvalidLocator,
                "path_escape",
                "resolved blob path escapes the configured MailVault archive root",
            );
        }

        let file = match File::open(&canonical_candidate) {
            Ok(file) => file,
            Err(error) => {
                return unavailable_from_io(item, expected_locator, "open", &error);
            }
        };
        let metadata = match file.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                return unavailable_from_io(item, expected_locator, "metadata", &error);
            }
        };
        if !metadata.is_file() {
            return FileStatObservation::unavailable(
                item,
                expected_locator,
                AvailabilityState::NonRegular,
                "non_regular",
                "resolved MailVault blob object is not a regular file",
            );
        }

        let actual_size_bytes = metadata.len();
        let size_state = if actual_size_bytes == item.expected_size_bytes {
            SizeState::Match
        } else {
            SizeState::Mismatch
        };

        FileStatObservation {
            content_object_id: item.content_object_id.clone(),
            sha256: item.sha256.clone(),
            source_locator: item.source_locator.clone(),
            expected_locator,
            availability_state: AvailabilityState::Available,
            size_state,
            expected_size_bytes: item.expected_size_bytes,
            actual_size_bytes: Some(actual_size_bytes),
            modified_unix_ns: metadata.modified().ok().and_then(system_time_unix_ns),
            error_kind: None,
            error_message: None,
        }
    }
}

pub(crate) fn expected_blob_locator(sha256: &str) -> String {
    format!(
        "objects/blobs/sha256/{}/{}/{}",
        &sha256[0..2],
        &sha256[2..4],
        sha256
    )
}

fn validate_relative_locator(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("blob locator is empty".into());
    }
    if path.is_absolute() {
        return Err("blob locator is absolute".into());
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(format!(
                "blob locator contains a forbidden path component: {component:?}"
            ));
        }
    }
    Ok(())
}

fn inspect_path_chain(
    root: &Path,
    relative_path: &Path,
    item: &FileStatWorkItem,
    expected_locator: &str,
) -> Result<(), Box<FileStatObservation>> {
    let mut current = root.to_path_buf();
    let component_count = relative_path.components().count();
    for (index, component) in relative_path.components().enumerate() {
        let Component::Normal(segment) = component else {
            return Err(Box::new(FileStatObservation::unavailable(
                item,
                expected_locator.to_owned(),
                AvailabilityState::InvalidLocator,
                "invalid_component",
                "blob locator contains a non-normal path component",
            )));
        };
        current.push(segment);
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) => {
                return Err(Box::new(unavailable_from_io(
                    item,
                    expected_locator.to_owned(),
                    "symlink_metadata",
                    &error,
                )));
            }
        };
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(Box::new(FileStatObservation::unavailable(
                item,
                expected_locator.to_owned(),
                AvailabilityState::UnsafeReparsePoint,
                "reparse_point",
                "MailVault object path traverses a symbolic link or reparse point",
            )));
        }
        let is_last = index + 1 == component_count;
        if !is_last && !metadata.is_dir() {
            return Err(Box::new(FileStatObservation::unavailable(
                item,
                expected_locator.to_owned(),
                AvailabilityState::NonRegular,
                "non_directory_parent",
                "a parent component of the MailVault object path is not a directory",
            )));
        }
    }
    Ok(())
}

fn unavailable_from_io(
    item: &FileStatWorkItem,
    expected_locator: String,
    operation: &str,
    error: &io::Error,
) -> FileStatObservation {
    let availability_state = match error.kind() {
        io::ErrorKind::NotFound => AvailabilityState::Missing,
        io::ErrorKind::PermissionDenied => AvailabilityState::Unreadable,
        _ => AvailabilityState::IoError,
    };
    FileStatObservation::unavailable(
        item,
        expected_locator,
        availability_state,
        format!("{operation}_{:?}", error.kind()).to_lowercase(),
        error.to_string(),
    )
}

fn system_time_unix_ns(value: SystemTime) -> Option<i64> {
    match value.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_nanos()).ok(),
        Err(error) => i64::try_from(error.duration().as_nanos())
            .ok()
            .and_then(i64::checked_neg),
    }
}

#[cfg(windows)]
fn is_reparse_point(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
const fn is_reparse_point(_metadata: &Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;

    fn item(root: &Path, payload: &[u8]) -> FileStatWorkItem {
        let sha256 = hex::encode(Sha256::digest(payload));
        let locator = expected_blob_locator(&sha256);
        let path = root.join(&locator);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, payload).unwrap();
        FileStatWorkItem {
            content_object_id: "object-1".into(),
            sha256,
            expected_size_bytes: payload.len() as u64,
            source_locator: locator,
        }
    }

    #[test]
    fn available_file_is_size_verified() {
        let directory = tempdir().unwrap();
        let item = item(directory.path(), b"mailvault");
        let resolver = MailVaultPhysicalObjectResolver::new(directory.path()).unwrap();
        let observation = resolver.inspect(directory.path(), &item);
        assert_eq!(observation.availability_state, AvailabilityState::Available);
        assert_eq!(observation.size_state, SizeState::Match);
        assert_eq!(observation.actual_size_bytes, Some(9));
    }

    #[test]
    fn locator_mismatch_is_never_opened() {
        let directory = tempdir().unwrap();
        let mut item = item(directory.path(), b"mailvault");
        item.source_locator = "objects/blobs/sha256/00/00/not-the-hash".into();
        let resolver = MailVaultPhysicalObjectResolver::new(directory.path()).unwrap();
        let observation = resolver.inspect(directory.path(), &item);
        assert_eq!(
            observation.availability_state,
            AvailabilityState::InvalidLocator
        );
    }

    #[test]
    fn missing_file_is_reported_without_panicking() {
        let directory = tempdir().unwrap();
        let item = item(directory.path(), b"mailvault");
        fs::remove_file(directory.path().join(&item.source_locator)).unwrap();
        let resolver = MailVaultPhysicalObjectResolver::new(directory.path()).unwrap();
        let observation = resolver.inspect(directory.path(), &item);
        assert_eq!(observation.availability_state, AvailabilityState::Missing);
    }
}
