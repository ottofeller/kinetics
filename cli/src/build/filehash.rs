use eyre::Context;
use std::collections::HashMap;
use std::fs::{self, File};
use std::hash::Hasher;
use std::io::Read;
use std::path::{Path, PathBuf};
use twox_hash::XxHash64;

/// Stores files hashes on the disk to avoid rebuilding on unchanged files.
/// NOTE: `cargo lambda` rebuilds crate if file timestamp changed.
pub struct FileHash {
    path: PathBuf,
    pub inner: HashMap<PathBuf, String>,
}

impl FileHash {
    pub fn new(dst: PathBuf) -> Self {
        let path = dst.join(".checksums");

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
            .is_some_and(|old_hash| new_hash != old_hash)
    }

    pub fn hash_from_path<P: AsRef<Path>>(path: P) -> eyre::Result<String> {
        let mut file = File::open(path).wrap_err("Failed to open file")?;
        let mut hasher = XxHash64::default();
        let mut buffer = [0; 8192];

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.write(&buffer[..bytes_read]);
        }

        Ok(format!("{:x}", hasher.finish()))
    }

    pub fn hash_from_bytes<C: AsRef<[u8]>>(contents: C) -> eyre::Result<String> {
        let mut hasher = XxHash64::default();
        hasher.write(contents.as_ref());
        Ok(format!("{:x}", hasher.finish()))
    }
}
