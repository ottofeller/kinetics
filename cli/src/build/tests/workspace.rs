use super::utils::{age_files, copy_fixture, files, fixture_path, replace, write, Fixture};
use crate::build::files::CHECKSUMS_FILENAME;
use crate::build::generate_workspace;
use crate::project::workspace::PackageReference;
use kinetics_parser::{Package, Params, Role};
use std::collections::BTreeMap;
use std::fs;

#[test]
fn generates_all_roles_and_preserves_dependency_settings() {
    let fixture = Fixture::new("sample");
    fs::copy(
        fixture_path("variants/all_roles.rs"),
        fixture.source.path().join("src/jobs.rs"),
    )
    .unwrap();
    let functions = fixture.generate();
    assert_eq!(functions.len(), 4);
    assert!(functions.iter().all(|function| function.is_deploying));
    for function in &functions {
        for suffix in ["", "Local"] {
            let code = fixture.read(&format!("src/bin/{}{suffix}.rs", function.name));
            syn::parse_file(&code).unwrap();
            assert!(code.contains("as __kinetics_user_function;"));
        }
    }
    let worker = functions
        .iter()
        .find(|function| function.role == Role::Worker)
        .unwrap();
    let Params::Worker(params) = &worker.params else {
        panic!("expected worker")
    };
    assert_eq!(params.dlq.as_deref(), Some("SampleJobsRetry"));
    let manifest = fixture.manifest("Cargo.toml");
    assert_eq!(manifest["lib"]["name"].as_str(), Some("sample"));
    assert_eq!(
        manifest["workspace"]["members"].as_array().unwrap(),
        &[toml::Value::String(".".into())]
    );
    let dependencies = &manifest["dependencies"];
    assert!(dependencies.get("kinetics-macro").is_none());
    assert_eq!(
        dependencies["kinetics-lib"]["version"].as_str(),
        Some(env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(dependencies["tokio"]["version"].as_str(), Some("1"));
    assert_eq!(
        dependencies["tokio"]["features"].as_array().unwrap(),
        &[
            toml::Value::String("sync".into()),
            toml::Value::String("full".into()),
        ]
    );
    assert!(dependencies.get("lambda_http").is_some());
    assert!(dependencies.get("lambda_runtime").is_some());
}

#[test]
fn unchanged_generation_preserves_contents_and_timestamps() {
    let fixture = Fixture::new("sample");
    fixture.generate();
    age_files(fixture.build.path());
    let before = files(fixture.build.path());
    fixture.generate();
    assert_eq!(before, files(fixture.build.path()));
}

#[test]
fn changing_one_handler_only_rewrites_its_isolated_source() {
    let fixture = Fixture::new("sample");
    fixture.generate();
    age_files(fixture.build.path());
    let before = files(fixture.build.path());
    replace(
        fixture.source.path(),
        "src/jobs.rs",
        "let _ = 1",
        "let _ = 2",
    );
    fixture.generate();
    let after = files(fixture.build.path());
    let changed: Vec<_> = after
        .iter()
        .filter(|(path, value)| before.get(*path) != Some(value))
        .map(|(path, _)| path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(changed, [".kinetics/handlers/src/jobs.rs"]);
    assert_eq!(before.len(), after.len());
}

#[test]
fn missing_outputs_are_restored_even_when_checksums_match() {
    let fixture = Fixture::new("sample");
    fixture.generate();
    let before = files(fixture.build.path());
    for path in [
        "assets/payload.txt",
        ".kinetics/handlers/src/jobs.rs",
        "src/bin/SampleJobsWorker.rs",
        "Cargo.toml",
    ] {
        fs::remove_file(fixture.build.path().join(path)).unwrap();
    }
    fixture.generate();
    for (path, (content, _)) in before {
        assert_eq!(content, fs::read(fixture.build.path().join(path)).unwrap());
    }
}

#[test]
fn removes_obsolete_outputs_but_preserves_cargo_cache_and_lockfile() {
    let fixture = Fixture::new("sample");
    fixture.generate();
    write(fixture.build.path(), "target/debug/cache", "cached build");
    write(fixture.build.path(), "Cargo.lock", "generated lockfile");
    replace(fixture.source.path(), "src/lib.rs", "mod jobs;", "");
    fs::remove_file(fixture.source.path().join("src/jobs.rs")).unwrap();
    fs::remove_file(fixture.source.path().join("assets/payload.txt")).unwrap();
    fixture.generate();
    for path in [
        "src/jobs.rs",
        ".kinetics/handlers/src/jobs.rs",
        "src/bin/SampleJobsWorker.rs",
        "src/bin/SampleJobsWorkerLocal.rs",
        "assets",
    ] {
        assert!(!fixture.build.path().join(path).exists(), "{path}");
    }
    assert_eq!(fixture.read("target/debug/cache"), "cached build");
    assert_eq!(fixture.read("Cargo.lock"), "generated lockfile");
    let checksums: BTreeMap<String, String> =
        serde_json::from_str(&fixture.read(CHECKSUMS_FILENAME)).unwrap();
    assert!(!checksums
        .keys()
        .any(|path| path.contains("jobs") || path.contains("Jobs")));
}

#[test]
fn skips_source_caches_and_rejects_invalid_attributes_before_writing() {
    let fixture = Fixture::new("sample");
    for directory in ["target", ".git", ".github", ".zig-cache"] {
        write(
            fixture.source.path(),
            &format!("{directory}/marker"),
            "ignored",
        );
    }
    fixture.generate();
    for directory in ["target", ".git", ".github", ".zig-cache"] {
        assert!(!fixture.build.path().join(directory).exists());
    }
    let before = files(fixture.build.path());
    fs::copy(
        fixture_path("variants/invalid.rs"),
        fixture.source.path().join("src/jobs.rs"),
    )
    .unwrap();
    let error = generate_workspace(&fixture.project, &[], None, fixture.build.path()).unwrap_err();
    assert!(error
        .to_string()
        .contains("Missing required attribute `url_path`"));
    assert_eq!(before, files(fixture.build.path()));
}

/// Add workspace members and a renamed dependency, optionally retaining a root package.
fn workspace_fixture(has_root_package: bool) -> Fixture {
    let mut fixture = Fixture::new("sample");
    let mut manifest: toml_edit::DocumentMut =
        fs::read_to_string(fixture.source.path().join("Cargo.toml"))
            .unwrap()
            .parse()
            .unwrap();
    copy_fixture("workspace", fixture.source.path());
    if has_root_package {
        let workspace: toml_edit::DocumentMut =
            fs::read_to_string(fixture.source.path().join("Cargo.toml"))
                .unwrap()
                .parse()
                .unwrap();
        manifest["workspace"] = workspace["workspace"].clone();
        write(fixture.source.path(), "Cargo.toml", &manifest.to_string());
    } else {
        fs::remove_dir_all(fixture.source.path().join("src")).unwrap();
        fixture.project.workspace.packages.clear();
    }
    for name in ["service", "consumer"] {
        fixture.project.workspace.packages.push(Package {
            name: name.into(),
            relative_path: name.into(),
        });
    }
    fixture.project.workspace.references.push(PackageReference {
        source: "consumer".into(),
        package: "service".into(),
        name: "renamed".into(),
    });
    fixture.project.workspace.is_standalone_crate = false;
    fixture
}

#[test]
fn workspace_members_and_external_references_are_preserved() {
    for has_root_package in [false, true] {
        let fixture = workspace_fixture(has_root_package);
        let functions = fixture.generate();
        assert_eq!(functions.len(), if has_root_package { 3 } else { 1 });
        let manifest = fixture.manifest("Cargo.toml");
        let expected = if has_root_package {
            vec![".", "service", "consumer"]
        } else {
            vec!["service", "consumer"]
        };
        assert_eq!(
            manifest["workspace"]["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(|member| member.as_str().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(!fixture
            .build
            .path()
            .join("service/.kinetics/handlers/src/lib.rs")
            .exists());
        assert!(fixture.read("service/src/lib.rs").contains("pub fn worker"));
        assert_eq!(
            fixture.read("consumer/src/lib.rs"),
            fs::read_to_string(fixture.source.path().join("consumer/src/lib.rs")).unwrap(),
        );
        age_files(fixture.build.path());
        let before = files(fixture.build.path());
        fixture.generate();
        assert_eq!(before, files(fixture.build.path()));
    }
}

#[test]
fn package_selection_marks_functions_without_removing_other_binaries() {
    let fixture = workspace_fixture(true);
    let functions = generate_workspace(
        &fixture.project,
        &["ServiceLibWorker".into()],
        Some("service"),
        fixture.build.path(),
    )
    .unwrap();
    assert_eq!(functions.len(), 3);
    let selected: Vec<_> = functions
        .iter()
        .filter(|function| function.is_deploying)
        .map(|function| function.name.as_str())
        .collect();
    assert_eq!(selected, ["ServiceLibWorker"]);
    assert!(fixture
        .build
        .path()
        .join("src/bin/SampleJobsWorker.rs")
        .exists());
    assert!(generate_workspace(
        &fixture.project,
        &["missing".into()],
        Some("service"),
        fixture.build.path()
    )
    .unwrap_err()
    .to_string()
    .contains("No functions match"));
    assert!(
        generate_workspace(&fixture.project, &[], Some("missing"), fixture.build.path())
            .unwrap_err()
            .to_string()
            .contains("was not found")
    );
}

#[test]
fn member_project_only_discovers_its_own_handlers() {
    let mut fixture = workspace_fixture(true);
    fixture.project.path = fixture.source.path().join("service");
    let functions = fixture.generate();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].name, "ServiceLibWorker");
    assert!(!fixture.build.path().join("src/bin").exists());
    assert!(
        generate_workspace(&fixture.project, &[], Some("sample"), fixture.build.path())
            .unwrap_err()
            .to_string()
            .contains("outside the current Kinetics project")
    );
}
