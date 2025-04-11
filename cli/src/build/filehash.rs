use std::collections::HashMap;
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use twox_hash::XxHash64;

pub const CHECKSUMS_FILENAME: &str = ".checksums";

/// Stores files hashes on the disk to avoid rebuilding on unchanged files.
/// NOTE: `cargo lambda` rebuilds crate if file timestamp changed.
pub struct FileHash {
    path: PathBuf,
    pub inner: HashMap<PathBuf, String>,
}

impl FileHash {
    pub fn new(dst: PathBuf) -> Self {
        let path = dst.join(CHECKSUMS_FILENAME);

        // Relative path -> hash of the file
        let checksums: HashMap<PathBuf, String> = {
            match fs::read_to_string(&path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => HashMap::new(),
            }
        };

        FileHash {
            inner: checksums,
            path,
        }
    }

    pub fn has_folder(&self, path: &Path) -> bool {
        self.inner
            .iter()
            .find_map(|(key, _hash)| key.strip_prefix(path).ok())
            .is_some()
    }

    pub fn has_file(&self, path: &Path) -> bool {
        self.inner.contains_key(path)
    }

    pub fn save(&self) -> eyre::Result<()> {
        Ok(fs::write(
            &self.path,
            serde_json::to_string_pretty(&self.inner)?,
        )?)
    }

    /// Insert a value into the checksum map.
    /// Returns:
    /// - 'true' if the value was updated;
    /// - 'false' is the value did not exist or existed but was not updated.
    pub fn update(&mut self, path: PathBuf, new_hash: &str) -> bool {
        self.inner
            .insert(path, new_hash.to_owned())
            .is_none_or(|old_hash| new_hash != old_hash)
    }

    pub fn hash_from_bytes<C: AsRef<[u8]>>(contents: C) -> eyre::Result<String> {
        let mut hasher = XxHash64::default();
        hasher.write(contents.as_ref());
        Ok(format!("{:x}", hasher.finish()))
    }
}
