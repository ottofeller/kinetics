use eyre::WrapErr;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::Hasher;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use twox_hash::XxHash64;
use walkdir::WalkDir;

pub const CHECKSUMS_FILENAME: &str = ".checksums";

/// Tracks build inputs and checksums to preserve timestamps and Cargo freshness.
pub(super) struct BuildFiles {
    directory: PathBuf,
    checksums: HashMap<PathBuf, String>,
    current: HashSet<PathBuf>,
}

impl BuildFiles {
    /// Load cached checksums; missing or invalid cache data starts an empty cache.
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

        BuildFiles {
            checksums,
            directory: dst,
            current: HashSet::new(),
        }
    }

    /// Mark an input as present in this generation pass so pruning keeps it.
    pub(super) fn register(&mut self, path: PathBuf) {
        self.current.insert(path);
    }

    /// Check whether this pass registered any input beneath the directory.
    pub fn has_folder(&self, path: &Path) -> bool {
        self.current.iter().any(|file| file.starts_with(path))
    }

    /// Check whether this pass registered the input, regardless of filesystem state.
    pub fn has_file(&self, path: &Path) -> bool {
        self.current.contains(path)
    }

    /// Persist checksums only for inputs registered in this pass.
    pub fn save(&self) -> eyre::Result<()> {
        let checksums: HashMap<_, _> = self
            .checksums
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
        let previous = self.checksums.insert(path.clone(), new_hash.to_owned());
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

    /// Copy changed sources, skipping build caches and caller-supplied paths.
    pub fn copy_tree(&mut self, src: &Path, relative: &Path, skip: &[PathBuf]) -> eyre::Result<()> {
        let destination = self.directory.join(relative);
        fs::create_dir_all(&destination)
            .wrap_err("Failed to create dir to clone the project to")?;
        let skip_paths: Vec<_> = ["target", ".git", ".github", ".zig-cache"]
            .into_iter()
            .map(|path| src.join(path))
            .chain(skip.iter().map(|path| src.join(path)))
            .collect();

        for entry in WalkDir::new(src).into_iter().filter_entry(|entry| {
            !skip_paths
                .iter()
                .any(|prefix| entry.path().starts_with(prefix))
        }) {
            let entry =
                entry.wrap_err_with(|| format!("Failed to read source directory {src:?}"))?;
            let src_path = entry.path();
            let path = relative.join(src_path.strip_prefix(src)?);
            let destination = self.directory.join(&path);
            if src_path.is_dir() {
                fs::create_dir_all(&destination).wrap_err("Create dir failed")?;
            } else if self.update(path, &Self::hash_from_file(src_path)?) {
                fs::copy(src_path, &destination).wrap_err_with(|| {
                    format!("Failed to copy file {src_path:?} -> {destination:?}")
                })?;
            }
        }
        Ok(())
    }

    /// Remove inputs not registered in this pass, then persist their checksums.
    pub fn prune_and_save(&self) -> eyre::Result<()> {
        let mut entries = WalkDir::new(&self.directory).into_iter();
        while let Some(entry) = entries.next() {
            let entry = entry
                .wrap_err_with(|| format!("Failed to read build directory {:?}", self.directory))?;
            let path = entry.path();
            let relative = path.strip_prefix(&self.directory)?;
            if relative.as_os_str().is_empty() {
                continue;
            }
            // Cargo owns the build cache and may create a lockfile itself.
            if relative.starts_with("target")
                || relative == Path::new(CHECKSUMS_FILENAME)
                || relative == Path::new("Cargo.lock")
            {
                if entry.file_type().is_dir() {
                    entries.skip_current_dir();
                }
                continue;
            }
            if entry.file_type().is_dir() {
                if !self.has_folder(relative) {
                    entries.skip_current_dir();
                    fs::remove_dir_all(path).wrap_err_with(|| {
                        format!("Failed to delete an obsolete folder {relative:?}")
                    })?;
                }
            } else if !self.has_file(relative) {
                fs::remove_file(path)
                    .wrap_err_with(|| format!("Failed to delete an obsolete file {relative:?}"))?;
            }
        }
        self.save().wrap_err("Failed to save checksums")
    }

    /// Return the hexadecimal xxHash64 used for generated input checksums.
    pub fn hash_from_bytes<C: AsRef<[u8]>>(contents: C) -> eyre::Result<String> {
        let mut hasher = XxHash64::default();
        hasher.write(contents.as_ref());
        Ok(format!("{:x}", hasher.finish()))
    }

    /// Hash a file in bounded chunks using the same checksum format as generated inputs.
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
