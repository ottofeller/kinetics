use eyre::WrapErr;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::Hasher;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use twox_hash::XxHash64;

pub const CHECKSUMS_FILENAME: &str = ".checksums";

/// Stores files hashes on the disk to avoid rebuilding on unchanged files.
/// NOTE: `cargo lambda` rebuilds crate if file timestamp changed.
pub struct FileHash {
    directory: PathBuf,
    pub inner: HashMap<PathBuf, String>,
    current: HashSet<PathBuf>,
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
            directory: dst,
            current: HashSet::new(),
        }
    }

    pub(super) fn register(&mut self, path: PathBuf) {
        self.current.insert(path);
    }

    pub fn has_folder(&self, path: &Path) -> bool {
        self.current.iter().any(|file| file.starts_with(path))
    }

    pub fn has_file(&self, path: &Path) -> bool {
        self.current.contains(path)
    }

    pub fn save(&self) -> eyre::Result<()> {
        let checksums: HashMap<_, _> = self
            .inner
            .iter()
            .filter(|(path, _)| self.current.contains(*path))
            .collect();
        Ok(fs::write(
            self.directory.join(CHECKSUMS_FILENAME),
            serde_json::to_string_pretty(&checksums)?,
        )?)
    }

    /// Register a current file and update its checksum.
    /// Returns true when the contents changed or the destination is missing.
    pub fn update(&mut self, path: PathBuf, new_hash: &str) -> bool {
        self.register(path.clone());
        let missing = !self.directory.join(&path).exists();
        self.inner
            .insert(path, new_hash.to_owned())
            .is_none_or(|old_hash| new_hash != old_hash)
            || missing
    }

    pub fn hash_from_bytes<C: AsRef<[u8]>>(contents: C) -> eyre::Result<String> {
        let mut hasher = XxHash64::default();
        hasher.write(contents.as_ref());
        Ok(format!("{:x}", hasher.finish()))
    }

    pub fn hash_from_file(path: &Path) -> eyre::Result<String> {
        let mut file =
            fs::File::open(path).wrap_err_with(|| format!("Failed to open file {path:?}"))?;
        let mut hasher = XxHash64::default();
        let mut buffer = [0u8; 64 * 1024];

        loop {
            match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(bytes_read) => hasher.write(&buffer[..bytes_read]),
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => {
                    return Err(error).wrap_err_with(|| format!("Failed to read file {path:?}"));
                }
            }
        }

        Ok(format!("{:x}", hasher.finish()))
    }
}
