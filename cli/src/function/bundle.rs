use super::Function;
use eyre::WrapErr;
use std::fs::File;
use std::hash::Hasher;
use std::io::{self, BufReader, Read};
use std::path::Path;
use tempfile::NamedTempFile;
use twox_hash::XxHash64;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

const BINARY_NAME: &str = "bootstrap";
const BINARY_MODE: u32 = 0o100755;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Fingerprint {
    size: u64,
    hash: u64,
}

/// Ensures each function has an up-to-date deployment bundle.
/// Existing bundles are reused when their `bootstrap` entry matches the current binary.
pub(super) fn bundle_functions(functions: &[Function]) -> eyre::Result<()> {
    for function in functions {
        let bundle_path = function.bundle_path().wrap_err_with(|| {
            format!(
                "Failed to determine bundle path for function `{}`",
                function.name
            )
        })?;
        let binary_path = bundle_path.with_file_name(BINARY_NAME);

        if !is_bundle_current(&binary_path, &bundle_path).wrap_err_with(|| {
            format!("Failed to inspect bundle for function `{}`", function.name)
        })? {
            write_bundle_atomic(&binary_path, &bundle_path)
                .wrap_err_with(|| format!("Failed to bundle function `{}`", function.name))?;
        }
    }

    Ok(())
}

/// Checks the bundle layout, metadata, and contents against the current binary.
/// Missing or invalid archives are treated as cache misses and rebuilt.
fn is_bundle_current(binary_path: &Path, bundle_path: &Path) -> eyre::Result<bool> {
    let binary_fingerprint = fingerprint_file(binary_path)
        .wrap_err_with(|| format!("Failed to read binary at {}", binary_path.display()))?;

    let bundle_file = match File::open(bundle_path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            return Err(error)
                .wrap_err_with(|| format!("Failed to open bundle at {}", bundle_path.display()));
        }
        Err(error) => {
            return Err(error)
                .wrap_err_with(|| format!("Failed to open bundle at {}", bundle_path.display()));
        }
    };

    // A malformed archive is a stale cache entry, not a build failure
    let mut archive = match ZipArchive::new(bundle_file) {
        Ok(archive) => archive,
        Err(_) => return Ok(false),
    };

    if archive.len() != 1 {
        return Ok(false);
    }

    let mut entry = match archive.by_index(0) {
        Ok(entry) => entry,
        Err(_) => return Ok(false),
    };

    // Accept only the exact layout expected by the Lambda runtime
    if entry.name() != BINARY_NAME
        || entry.compression() != CompressionMethod::Deflated
        || entry.unix_mode() != Some(BINARY_MODE)
        || entry.size() != binary_fingerprint.size
    {
        return Ok(false);
    }

    let bundle_fingerprint = match fingerprint_reader(&mut entry) {
        Ok(fingerprint) => fingerprint,
        Err(_) => return Ok(false),
    };

    Ok(bundle_fingerprint == binary_fingerprint)
}

/// Writes a function binary to a deployment bundle atomically.
/// The existing archive is replaced only after the new archive is complete and synced.
fn write_bundle_atomic(binary_path: &Path, bundle_path: &Path) -> eyre::Result<()> {
    let bundle_dir = bundle_path.parent().ok_or_else(|| {
        eyre::eyre!(
            "Bundle path has no parent directory: {}",
            bundle_path.display()
        )
    })?;
    let binary = File::open(binary_path)
        .wrap_err_with(|| format!("Failed to open binary at {}", binary_path.display()))?;

    // Keeping the temporary file beside the bundle allows an atomic replace.
    let temp_file = NamedTempFile::new_in(bundle_dir).wrap_err_with(|| {
        format!(
            "Failed to create temporary bundle in {}",
            bundle_dir.display()
        )
    })?;

    let mut archive = ZipWriter::new(temp_file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        // A fixed timestamp makes archives deterministic for identical input.
        .last_modified_time(DateTime::default())
        .unix_permissions(BINARY_MODE);

    archive
        .start_file(BINARY_NAME, options)
        .wrap_err("Failed to start bootstrap ZIP entry")?;

    io::copy(&mut BufReader::new(binary), &mut archive)
        .wrap_err_with(|| format!("Failed to read binary at {}", binary_path.display()))?;

    let mut temp_file = archive.finish().wrap_err("Failed to finish bundle")?;
    // Sync before persist so a write failure cannot replace a valid old bundle.
    temp_file
        .as_file_mut()
        .sync_all()
        .wrap_err("Failed to sync temporary bundle")?;

    temp_file
        .persist(bundle_path)
        .map_err(|error| error.error)
        .wrap_err_with(|| format!("Failed to persist bundle at {}", bundle_path.display()))?;

    Ok(())
}

/// Opens a file and computes its content fingerprint without buffering the
/// entire file in memory.
fn fingerprint_file(path: &Path) -> io::Result<Fingerprint> {
    fingerprint_reader(&mut BufReader::new(File::open(path)?))
}

/// Streams readable data into xxHash64 and records its total byte length.
/// The fingerprint is intended for local cache invalidation.
fn fingerprint_reader(reader: &mut impl Read) -> io::Result<Fingerprint> {
    let mut hasher = XxHash64::default();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];

    // Hash in fixed-size chunks so memory use stays bounded for large binaries.
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        hasher.write(&buffer[..bytes_read]);
        size += bytes_read as u64;
    }

    Ok(Fingerprint {
        size,
        hash: hasher.finish(),
    })
}
