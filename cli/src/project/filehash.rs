use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use twox_hash::XxHash64;

pub const CHECKSUMS_FILENAME: &str = ".checksums";

/// Stores files hashes on the disk to avoid rebuilding on unchanged files.
/// NOTE: `cargo lambda` rebuilds crate if file timestamp changed.
pub struct FileHash {
    path: PathBuf,
    previous: HashMap<PathBuf, String>,
    current: BTreeMap<PathBuf, String>,
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
            previous: checksums,
            path,
            current: BTreeMap::new(),
        }
    }

    pub fn has_folder(&self, path: &Path) -> bool {
        self.current
            .keys()
            .find_map(|key| key.strip_prefix(path).ok())
            .is_some()
    }

    pub fn has_file(&self, path: &Path) -> bool {
        self.current.contains_key(path)
    }

    pub fn save(&self) -> eyre::Result<()> {
        let content = serde_json::to_vec_pretty(&self.current)?;
        if fs::read(&self.path).is_ok_and(|existing| existing == content) {
            return Ok(());
        }

        Ok(fs::write(&self.path, content)?)
    }

    /// Insert a value into the checksum map.
    /// Returns:
    /// - 'true' if the value was updated;
    /// - 'false' if the value existed and was not updated.
    pub fn update(&mut self, path: PathBuf, new_hash: &str) -> bool {
        let destination_exists = self
            .path
            .parent()
            .is_some_and(|root| root.join(&path).is_file());
        let changed = self
            .previous
            .get(&path)
            .is_none_or(|old_hash| new_hash != old_hash);

        self.current.insert(path, new_hash.to_owned());
        changed || !destination_exists
    }

    pub fn hash_from_bytes<C: AsRef<[u8]>>(contents: C) -> eyre::Result<String> {
        let mut hasher = XxHash64::default();
        hasher.write(contents.as_ref());
        Ok(format!("{:x}", hasher.finish()))
    }
}
