use super::utils::{compile_and_run, parsed, source};
use crate::build::sources::PackageSources;
use kinetics_parser::ParsedPackage;
use std::path::Path;

fn generate(parsed: &ParsedPackage) -> PackageSources {
    PackageSources::generate(parsed, Path::new("src/lib.rs"), "sample".into(), &[]).unwrap()
}

#[test]
fn private_helpers_and_multiple_handlers_move_together() {
    let parsed = parsed("private_helpers");
    let generated = generate(&parsed);
    assert_eq!(generated.handlers.len(), 1);
    assert!(generated.shared[Path::new("src/jobs.rs")].items.is_empty());
    compile_and_run(&generated, &parsed, "src/lib.rs", "worker", 42);
    compile_and_run(&generated, &parsed, "src/lib.rs", "retry", 42);
}

#[test]
fn shared_types_and_parent_helpers_remain_accessible() {
    let parsed = parsed("shared_types");
    let generated = generate(&parsed);
    assert!(generated
        .handlers
        .contains_key(Path::new("src/api/nested.rs")));
    let shared = source(&generated.shared, "src/api/nested.rs");
    assert!(shared.contains("pub number: i32"));
    assert!(!shared.contains("fn request"));
    compile_and_run(&generated, &parsed, "src/lib.rs", "request", 42);
}

#[test]
fn root_handler_and_custom_library_path_compile() {
    for lib_path in ["src/lib.rs", "src/custom.rs"] {
        let mut parsed = parsed("root_handler");
        let syntax = parsed.sources.remove(Path::new("src/lib.rs")).unwrap();
        parsed.sources.insert(lib_path.into(), syntax);
        parsed.functions[0].relative_path = lib_path.into();
        let generated =
            PackageSources::generate(&parsed, Path::new(lib_path), "sample".into(), &[]).unwrap();
        assert_eq!(generated.handlers.len(), 1);
        assert!(source(&generated.shared, lib_path).contains("fn helper"));
        compile_and_run(&generated, &parsed, lib_path, "worker", 42);
    }
}

#[test]
fn references_from_other_modules_keep_handlers_shared() {
    for reference in [
        "fn call() { crate::jobs::worker(); }",
        "use crate::jobs::worker as job; fn call() { job(); }",
        "use crate::jobs::*; fn call() { worker(); }",
        "mod nested { fn call() { super::super::jobs::worker(); } }",
        "fn call() { stringify!(crate::jobs::worker); }",
    ] {
        let mut parsed = parsed("references");
        parsed
            .sources
            .insert("src/caller.rs".into(), syn::parse_file(reference).unwrap());
        let generated = generate(&parsed);
        assert!(generated.handlers.is_empty(), "{reference}");
        assert!(source(&generated.shared, "src/jobs.rs").contains("pub fn worker"));
        assert_eq!(
            generated.render_handler_import(Path::new("/unused"), &parsed.functions[0]),
            "use sample::jobs::worker as __kinetics_user_function;"
        );
    }
}

#[test]
fn external_package_references_respect_dependency_aliases() {
    let parsed = parsed("external_references");
    for reference in [
        "use renamed::jobs::worker;",
        "use renamed::jobs::*;",
        "fn call() { sample::jobs::worker(); }",
    ] {
        let mut external = super::utils::parsed("external_caller");
        external
            .sources
            .insert("src/lib.rs".into(), syn::parse_file(reference).unwrap());
        let generated = PackageSources::generate(
            &parsed,
            Path::new("src/lib.rs"),
            "sample".into(),
            &[(&external, "renamed")],
        )
        .unwrap();
        assert!(generated.handlers.is_empty(), "{reference}");
    }
}

#[test]
fn shared_helper_referencing_its_handler_keeps_the_file_shared() {
    let parsed = parsed("shared_helper");
    let generated = generate(&parsed);
    assert!(generated.handlers.is_empty());
    compile_and_run(&generated, &parsed, "src/lib.rs", "worker", 42);
}

#[test]
fn macro_references_from_shared_helpers_keep_handlers_shared() {
    for expression in [
        "dbg!(worker())",
        "dbg!({ [worker()][0] })",
        "dbg!(self::worker())",
        "dbg!(crate::jobs::worker())",
    ] {
        let mut parsed = parsed("shared_helper");
        // A public helper stays in the library, so its macro must still reach the worker.
        let code = format!(
            "pub fn call() -> i32 {{ {expression} }} #[worker] fn worker() -> i32 {{ 42 }}"
        );
        parsed
            .sources
            .insert("src/jobs.rs".into(), syn::parse_file(&code).unwrap());
        let generated = generate(&parsed);
        assert!(generated.handlers.is_empty(), "{expression}");
        // Compile both the shared library and its handler binary to catch missing symbols.
        compile_and_run(&generated, &parsed, "src/lib.rs", "worker", 42);
    }
}

#[test]
fn macro_string_literals_do_not_block_handler_isolation() {
    for literal in [
        r#""worker from_worker crate::jobs::worker""#,
        r##"r#"worker from_worker crate::jobs::worker"#"##,
    ] {
        let mut parsed = parsed("shared_helper");
        let code = format!(
            "pub fn call() -> i32 {{ let _ = format!({literal}); 42 }} \
             #[worker] fn worker() -> i32 {{ 42 }}"
        );
        parsed
            .sources
            .insert("src/jobs.rs".into(), syn::parse_file(&code).unwrap());
        let generated = generate(&parsed);
        assert!(
            generated.handlers.contains_key(Path::new("src/jobs.rs")),
            "{literal}"
        );
        compile_and_run(&generated, &parsed, "src/lib.rs", "worker", 42);
    }
}

#[test]
fn includes_macro_items_and_unresolved_worker_identity_stay_shared() {
    for code in [
        r#"#[worker] fn worker() { include_str!("payload.txt"); }"#,
        "generate_items!(); #[worker] fn worker() {}",
        "#[worker] fn worker() { unknown::Client::from_worker(worker); }",
        "#[worker] fn worker() { stringify!(Client::from_worker(worker)); }",
    ] {
        let mut parsed = parsed("conservative");
        parsed
            .sources
            .insert("src/jobs.rs".into(), syn::parse_file(code).unwrap());
        assert!(generate(&parsed).handlers.is_empty(), "{code}");
    }
}

#[test]
fn private_macro_imports_are_not_exported() {
    let parsed = parsed("private_macro");
    let generated = generate(&parsed);
    assert!(generated.handlers.is_empty());
    assert!(source(&generated.shared, "src/lib.rs").contains("pub(crate) use answer;"));
    assert!(source(&generated.shared, "src/jobs.rs").contains("use crate::answer;"));
    assert!(!source(&generated.shared, "src/jobs.rs").contains("pub use crate::answer;"));
    compile_and_run(&generated, &parsed, "src/lib.rs", "worker", 42);
}

#[test]
fn local_macro_moves_with_its_handler() {
    let parsed = parsed("local_macro");
    let generated = generate(&parsed);
    assert_eq!(generated.handlers.len(), 1);
    compile_and_run(&generated, &parsed, "src/lib.rs", "worker", 42);
}

#[test]
fn known_worker_references_are_lowered_without_retaining_the_worker() {
    let parsed = parsed("queue_calls");
    let generated = generate(&parsed);
    assert_eq!(generated.handlers.len(), 2);
    let handler = source(&generated.handlers, "src/sender.rs");
    assert!(handler.contains(r#"Queue::from_name("sample", "jobs::worker")"#));
    assert!(handler.contains(r#"Queue::from_name("sample", "jobs::retry")"#));
    assert!(!handler.contains("use crate::jobs"));
    assert!(!handler.contains("from_worker"));
}

#[test]
fn shadowed_worker_reference_is_not_lowered() {
    let parsed = parsed("shadowed_worker");
    let generated = generate(&parsed);
    assert!(generated.handlers.is_empty());
    assert!(source(&generated.shared, "src/jobs.rs").contains("Client::from_worker(worker)"));
}

#[test]
fn changing_a_handler_or_private_helper_preserves_all_shared_sources() {
    let before = parsed("incremental");
    let original = source(&before.sources, "src/jobs.rs");
    let generated_before = generate(&before);
    for change in [
        original.replace("1", "2"),
        original.replace("helper()\n", "helper() + 1\n"),
    ] {
        let mut after = parsed("incremental");
        after
            .sources
            .insert("src/jobs.rs".into(), syn::parse_file(&change).unwrap());
        let generated_after = generate(&after);
        for path in generated_before.shared.keys() {
            assert_eq!(
                prettyplease::unparse(&generated_before.shared[path]),
                prettyplease::unparse(&generated_after.shared[path])
            );
        }
        assert_eq!(
            source(&generated_before.handlers, "src/other.rs"),
            source(&generated_after.handlers, "src/other.rs")
        );
        assert_ne!(
            source(&generated_before.handlers, "src/jobs.rs"),
            source(&generated_after.handlers, "src/jobs.rs")
        );
    }
}
