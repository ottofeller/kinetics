use crate::build::files::CHECKSUMS_FILENAME;
use crate::build::generate_workspace;
use crate::build::sources::PackageSources;
use crate::function::Function;
use crate::project::workspace::Workspace;
use crate::project::Project;
use kinetics_parser::{Package, ParsedPackage};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};
use syn::visit_mut::VisitMut;
use walkdir::WalkDir;

/// Write a fixture file, creating its parent directories.
pub(super) fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// Locate checked-in fixtures independently of the test's working directory.
pub(super) fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/build/tests/fixtures")
        .join(name)
}

/// Copy fixture files so tests can edit them without changing the originals.
pub(super) fn copy_fixture(name: &str, destination: &Path) {
    let source = fixture_path(name);
    for entry in WalkDir::new(&source) {
        let entry = entry.unwrap();
        let path = destination.join(entry.path().strip_prefix(&source).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(path).unwrap();
        } else {
            fs::copy(entry.path(), path).unwrap();
        }
    }
}

/// Apply a focused source edit and fail if the fixture no longer matches it.
pub(super) fn replace(root: &Path, path: &str, before: &str, after: &str) {
    let content = fs::read_to_string(root.join(path)).unwrap();
    assert!(
        content.contains(before),
        "{path} does not contain {before:?}"
    );
    write(root, path, &content.replace(before, after));
}

/// Parse a checked-in sample without invoking Cargo or creating files.
pub(super) fn parsed(name: &str) -> ParsedPackage {
    ParsedPackage::read(
        &fixture_path(&format!("sources/{name}")),
        Path::new("src/lib.rs"),
        Some(&Package {
            name: "sample".into(),
            relative_path: Default::default(),
        }),
    )
    .unwrap()
}

/// A private copy of a source project and its generated build workspace.
pub(super) struct Fixture {
    pub source: tempfile::TempDir,
    pub build: tempfile::TempDir,
    pub project: Project,
}

impl Fixture {
    /// Copy a standalone fixture into a temporary directory.
    pub fn new(name: &str) -> Self {
        let source = tempfile::tempdir().unwrap();
        copy_fixture(name, source.path());
        let build = tempfile::tempdir().unwrap();
        let project = Project {
            path: source.path().into(),
            workspace: Workspace {
                root_path: source.path().into(),
                is_standalone_crate: true,
                packages: vec![Package {
                    name: "sample".into(),
                    relative_path: PathBuf::new(),
                }],
                references: vec![],
            },
            name: "sample".into(),
            url: String::new(),
            kvdb: vec![],
            observability: None,
            domain_name: None,
            org: None,
        };
        Self {
            source,
            build,
            project,
        }
    }

    /// Run the production workspace generator against this fixture.
    pub fn generate(&self) -> Vec<Function> {
        generate_workspace(&self.project, &[], None, self.build.path()).unwrap()
    }

    /// Read a generated file for assertions.
    pub fn read(&self, path: &str) -> String {
        fs::read_to_string(self.build.path().join(path)).unwrap()
    }

    /// Parse a generated manifest for assertions.
    pub fn manifest(&self, path: &str) -> toml::Value {
        self.read(path).parse().unwrap()
    }
}

/// Builds generated sources with small host binaries to inspect real Cargo fingerprints.
pub(super) struct CargoFixture {
    pub project: Fixture,
    harness: tempfile::TempDir,
}

impl CargoFixture {
    /// Compile every target once to establish a warm cache.
    pub fn new() -> Self {
        let project = Fixture::new("incremental/project");
        let harness = tempfile::tempdir().unwrap();
        copy_fixture("incremental/harness", harness.path());
        replace(
            harness.path(),
            "Cargo.toml",
            "@GENERATED@",
            &project.build.path().to_string_lossy().replace('\\', "/"),
        );
        let fixture = Self { project, harness };
        fixture.generate();
        let targets = fixture.build(false);
        assert!(targets.values().all(|is_fresh| !is_fresh));
        fixture.assert_output("SampleApiRequest", "21");
        fixture.assert_output("SampleJobsWorker", "11");
        fixture.assert_output("SamplePairFirst", "31");
        fixture.assert_output("SamplePairSecond", "41");
        fixture
    }

    /// Keep production imports and file timestamps, substituting only the runtime entrypoint.
    fn generate(&self) {
        let functions = self.project.generate();
        let template = fs::read_to_string(fixture_path("incremental/main.rs")).unwrap();
        for function in functions {
            let path = format!("src/bin/{}.rs", function.name);
            let generated = self.project.read(&path);
            let marker = "as __kinetics_user_function;";
            let end = generated.find(marker).unwrap() + marker.len();
            let content = template.replace("// HANDLER_IMPORT", &generated[..end]);
            let binary = format!("bin/{}.rs", function.name);
            write(self.harness.path(), &binary, &content);
            // Propagate even accidental rewrites in the generator to Cargo's fingerprint.
            let modified = fs::metadata(self.project.build.path().join(path))
                .unwrap()
                .modified()
                .unwrap();
            fs::File::options()
                .write(true)
                .open(self.harness.path().join(binary))
                .unwrap()
                .set_modified(modified)
                .unwrap();
        }
    }

    /// Compile offline in a separate target directory and read Cargo's per-target freshness.
    fn build(&self, is_locked: bool) -> BTreeMap<String, bool> {
        let mut command = Command::new(env!("CARGO"));
        command
            .current_dir(self.harness.path())
            .args([
                "build",
                "--workspace",
                "--release",
                "--offline",
                "--message-format=json",
            ])
            .env("CARGO_TARGET_DIR", self.harness.path().join("target"))
            .env_remove("CARGO_BUILD_TARGET")
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER");
        if is_locked {
            command.arg("--locked");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout),
        );
        let targets: BTreeMap<_, _> =
            cargo_metadata::Message::parse_stream(output.stdout.as_slice())
                .filter_map(|message| match message.unwrap() {
                    cargo_metadata::Message::CompilerArtifact(artifact) => {
                        Some((artifact.target.name, artifact.fresh))
                    }
                    _ => None,
                })
                .collect();
        assert_eq!(
            targets.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "markers",
                "sample",
                "unrelated",
                "SampleApiRequest",
                "SampleJobsWorker",
                "SamplePairFirst",
                "SamplePairSecond",
            ]),
        );
        targets
    }

    /// Regenerate and require exactly the listed targets to compile again.
    pub fn assert_rebuilt(&self, expected: &[&str]) {
        self.generate();
        let targets = self.build(true);
        let rebuilt: BTreeSet<_> = targets
            .iter()
            .filter(|(_, is_fresh)| !**is_fresh)
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(rebuilt, expected.iter().copied().collect(), "{targets:?}");
    }

    /// Execute a compiled handler to catch stale artifacts as well as excessive rebuilds.
    pub fn assert_output(&self, binary: &str, expected: &str) {
        let output = Command::new(
            self.harness
                .path()
                .join("target/release")
                .join(format!("{binary}{}", std::env::consts::EXE_SUFFIX,)),
        )
        .output()
        .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), expected);
    }
}

/// Snapshot generated contents and timestamps, excluding the checksum cache.
pub(super) fn files(root: &Path) -> BTreeMap<PathBuf, (Vec<u8>, SystemTime)> {
    WalkDir::new(root)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file() && entry.file_name() != CHECKSUMS_FILENAME)
        .map(|entry| {
            (
                entry.path().strip_prefix(root).unwrap().to_owned(),
                (
                    fs::read(entry.path()).unwrap(),
                    entry.metadata().unwrap().modified().unwrap(),
                ),
            )
        })
        .collect()
}

/// Set old timestamps so an unnecessary rewrite is detectable without sleeping.
pub(super) fn age_files(root: &Path) {
    for path in files(root).keys() {
        fs::File::options()
            .write(true)
            .open(root.join(path))
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000))
            .unwrap();
    }
}

/// Format a generated source file for content assertions.
pub(super) fn source(files: &BTreeMap<PathBuf, syn::File>, path: &str) -> String {
    prettyplease::unparse(&files[Path::new(path)])
}

/// Removes workload attributes so fixture sources compile without procedural macros.
struct RemoveMarkers;

impl VisitMut for RemoveMarkers {
    fn visit_item_fn_mut(&mut self, function: &mut syn::ItemFn) {
        function.attrs.retain(|attr| {
            !["endpoint", "worker", "dlq_worker", "cron"]
                .iter()
                .any(|name| attr.path().is_ident(name))
        });
        syn::visit_mut::visit_item_fn_mut(self, function);
    }
}

/// Compile generated sources with rustc and assert the selected handler's return value.
pub(super) fn compile_and_run(
    generated: &PackageSources,
    parsed: &ParsedPackage,
    lib_path: &str,
    name: &str,
    expected: i32,
) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    for (prefix, sources) in [
        ("", &generated.shared),
        (".kinetics/handlers", &generated.handlers),
    ] {
        for (path, syntax) in sources {
            let mut syntax = syntax.clone();
            RemoveMarkers.visit_file_mut(&mut syntax);
            write(
                root,
                &Path::new(prefix).join(path).to_string_lossy(),
                &prettyplease::unparse(&syntax),
            );
        }
    }
    let library = root.join("libsample.rlib");
    let output = Command::new("rustc")
        .args(["--edition=2021", "--crate-type=lib", "--crate-name=sample"])
        .arg(root.join(lib_path))
        .arg("-o")
        .arg(&library)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let function = parsed
        .functions
        .iter()
        .find(|function| function.rust_function_name == name)
        .unwrap();
    write(
        root,
        "main.rs",
        &format!(
            "{}\nfn main() {{ assert_eq!(__kinetics_user_function(), {expected}); }}",
            generated.render_handler_import(root, function),
        ),
    );
    let executable = root.join("handler");
    let output = Command::new("rustc")
        .arg("--edition=2021")
        .arg(root.join("main.rs"))
        .arg("--extern")
        .arg(format!("sample={}", library.display()))
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(Command::new(executable).status().unwrap().success());
}
