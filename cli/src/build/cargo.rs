use crate::function::Function;
use eyre::{eyre, WrapErr};
use std::io::IsTerminal;
use std::process::Stdio;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Build selected Lambda binaries and report Cargo progress and errors.
pub(super) async fn compile(
    functions: &[Function],
    total_progress: &indicatif::ProgressBar,
) -> eyre::Result<()> {
    let Some(Function { project, .. }) = functions.iter().next() else {
        return Err(eyre!("Attempted to build an empty function list"));
    };

    total_progress.set_message("Starting cargo...");
    let build_path = project.build_path()?;
    let is_debug_logging_enabled = log::log_enabled!(log::Level::Debug);
    let is_build_output_enabled = is_debug_logging_enabled || !std::io::stderr().is_terminal();
    let mut cmd = tokio::process::Command::new("cargo");
    cmd.arg("lambda")
        .arg("build")
        .arg("--workspace")
        .arg("--release")
        .arg("--target")
        .arg("x86_64-unknown-linux-musl")
        .arg("--output-format")
        .arg("binary")
        .current_dir(&build_path)
        // Null stdout avoids an unread pipe when cargo-lambda inherits verbose RUST_LOG.
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    if is_debug_logging_enabled {
        // Cargo's verbose output includes Fresh/Dirty units and their rebuild reasons.
        cmd.arg("--verbose").arg("--timings");
    }

    for function in functions {
        cmd.arg("--bin").arg(&function.name);
    }

    log::debug!(
        "Building {} Lambda binaries in {}",
        functions.len(),
        build_path.display()
    );
    log::debug!("Running {cmd:?}");
    let compile_started_at = Instant::now();
    let mut child = cmd.spawn().wrap_err("Failed to execute the process")?;

    let mut is_failed = false;
    let mut error_message_lines = Vec::new();

    if let Some(stderr) = child.stderr.take() {
        let mut reader = BufReader::new(stderr).lines();
        let regex = regex::Regex::new(r"^[\t ]+")?;

        while let Some(line) = reader.next_line().await? {
            if is_build_output_enabled {
                total_progress.suspend(|| eprintln!("{line}"));
            }

            let trimmed = line.trim();

            if is_failed || trimmed.starts_with("error") || trimmed.starts_with("Error") {
                is_failed = true;
                error_message_lines.push(line);
                continue;
            };
            total_progress.set_message(regex.replace_all(&line, "").to_string());
        }
    }

    total_progress.set_message("");
    let status = child.wait().await?;
    log::debug!(
        "Cargo finished in {:.2}s ({status})",
        compile_started_at.elapsed().as_secs_f64()
    );

    if !status.success() {
        return Err(eyre!("{}", error_message_lines.join("\n")));
    }

    Ok(())
}
