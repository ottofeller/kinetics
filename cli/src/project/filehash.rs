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
                Ok(content) => serde_json::from_str(&content).unwrap_or_else(|error| {
                    log::debug!(
                        "Cannot parse cached build inputs {}: {error}",
                        path.display()
                    );
                    HashMap::new()
                }),
                Err(error) => {
                    log::debug!(
                        "Cannot load cached build inputs {}: {error}",
                        path.display()
                    );
                    HashMap::new()
                }
            }
        };
        log::debug!(
            "Loaded {} cached build input hashes from {}",
            checksums.len(),
            path.display()
        );

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
        let previous = self.inner.insert(path.clone(), new_hash.to_owned());
        let changed = previous.as_deref() != Some(new_hash);
        if missing || changed {
            log::debug!(
                "Updating build input {}: {}",
                path.display(),
                if missing {
                    "file missing"
                } else {
                    "contents changed"
                },
            );
        }
        missing || changed
    }

    /// Write a generated input without changing its timestamp when it is unchanged.
    pub(super) fn write(&mut self, path: &Path, content: impl AsRef<[u8]>) -> eyre::Result<()> {
        let content = content.as_ref();
        if self.update(path.to_path_buf(), &Self::hash_from_bytes(content)?) {
            let destination = self.directory.join(path);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&destination, content).wrap_err_with(|| {
                format!("Failed to write build input {}", destination.display())
            })?;
        }
        Ok(())
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
