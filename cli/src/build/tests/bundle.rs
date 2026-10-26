use crate::build::bundle::{is_cache_hit, write_bundle_atomic};
use std::fs::{self, File};
use std::io::{Read, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[test]
fn archive_contains_an_executable_bootstrap_with_the_binary_contents() {
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("bootstrap");
    let bundle = directory.path().join("bootstrap.zip");
    let contents = b"binary contents\0with non-text bytes\xff";
    fs::write(&binary, contents).unwrap();
    write_bundle_atomic(&binary, &bundle).unwrap();

    let mut archive = ZipArchive::new(File::open(&bundle).unwrap()).unwrap();
    assert_eq!(archive.len(), 1);
    let mut entry = archive.by_index(0).unwrap();
    assert_eq!(entry.name(), "bootstrap");
    assert_eq!(entry.unix_mode(), Some(0o100755));
    assert_eq!(entry.compression(), CompressionMethod::Deflated);
    let mut extracted = Vec::new();
    entry.read_to_end(&mut extracted).unwrap();
    assert_eq!(extracted, contents);
    assert!(is_cache_hit(&binary, &bundle).unwrap());
}

#[test]
fn identical_inputs_produce_identical_archives() {
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("bootstrap");
    let first = directory.path().join("first.zip");
    let second = directory.path().join("second.zip");
    fs::write(&binary, b"same binary").unwrap();
    write_bundle_atomic(&binary, &first).unwrap();
    write_bundle_atomic(&binary, &second).unwrap();
    assert_eq!(fs::read(first).unwrap(), fs::read(second).unwrap());
}

#[test]
fn changed_binary_invalidates_archive_even_when_size_matches() {
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("bootstrap");
    let bundle = directory.path().join("bootstrap.zip");
    fs::write(&binary, b"before").unwrap();
    write_bundle_atomic(&binary, &bundle).unwrap();
    assert!(is_cache_hit(&binary, &bundle).unwrap());
    fs::write(&binary, b"after!").unwrap();
    assert!(!is_cache_hit(&binary, &bundle).unwrap());
    write_bundle_atomic(&binary, &bundle).unwrap();
    assert!(is_cache_hit(&binary, &bundle).unwrap());
}

#[test]
fn missing_corrupt_and_incompatible_archives_are_cache_misses() {
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("bootstrap");
    let bundle = directory.path().join("bootstrap.zip");
    fs::write(&binary, b"binary").unwrap();
    assert!(!is_cache_hit(&binary, &bundle).unwrap());
    fs::write(&bundle, b"not a zip archive").unwrap();
    assert!(!is_cache_hit(&binary, &bundle).unwrap());

    for (name, mode, compression, extra) in [
        ("wrong-name", 0o755, CompressionMethod::Deflated, false),
        ("bootstrap", 0o644, CompressionMethod::Deflated, false),
        ("bootstrap", 0o755, CompressionMethod::Stored, false),
        ("bootstrap", 0o755, CompressionMethod::Deflated, true),
    ] {
        let mut archive = ZipWriter::new(File::create(&bundle).unwrap());
        let options = SimpleFileOptions::default()
            .compression_method(compression)
            .unix_permissions(mode);
        archive.start_file(name, options).unwrap();
        archive.write_all(b"binary").unwrap();
        if extra {
            archive.start_file("extra", options).unwrap();
        }
        archive.finish().unwrap();
        assert!(!is_cache_hit(&binary, &bundle).unwrap());
    }
}
