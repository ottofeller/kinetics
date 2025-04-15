use eyre::{ContextCompat, WrapErr};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::crat::Crate;
use crate::function::Function;
use crate::secret::Secret;

/// Create a thread for printing out a line, and accumulating for later full output
fn thread(
    reader: BufReader<impl Read + Send + 'static>,
    lock: Arc<Mutex<Vec<String>>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for line in reader.lines() {
            if let Ok(line) = line {
                // Store the line for later output
                if let Ok(mut lines) = lock.lock() {
                    lines.push(line.clone());
                }

                // Clear the line and print normal (non-red) output
                print!("\r\x1B[K");
                print!("{}", console::style(&line.trim()).dim());
                let _ = std::io::stdout().flush();
            }
        }
    })
}

/// Invoke the function locally
///
/// Crate is the original crate that the function belongs to. function.crate is the one where the function
/// was copied to, it does not preserve the original name.
pub fn invoke(function: &Function, crat: &Crate) -> eyre::Result<()> {
    let home = std::env::var("HOME").wrap_err("Can not read HOME env var")?;

    // Load secrets from .env.secrets if it exists
    let mut secrets = HashMap::new();

    for secret in Secret::from_dotenv().wrap_err("Failed to read secrets")? {
        secrets.insert(
            format!("KINETICS_SECRET_{}", secret.name.clone()),
            secret.value(),
        );
    }

    let invoke_dir =
        Path::new(&home).join(format!(".kinetics/{}/{}Local", crat.name, function.name()?));

    let display_path = format!("{}", invoke_dir.display());

    println!(
        "\n{} from {}",
        console::style("Invoking function").green().bold(),
        console::style(&display_path).underlined().bold()
    );

    // Collect all lines for later display
    let stdout_lines = Arc::new(Mutex::new(Vec::new()));
    let stderr_lines = Arc::new(Mutex::new(Vec::new()));

    // Start the command with piped stdout and stderr
    let mut child = Command::new("cargo")
        .args(["run"])
        .envs(secrets)
        .current_dir(&invoke_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .wrap_err("Failed to execute cargo run")?;

    // Create readers for stdout and stderr
    let stdout = child.stdout.take().wrap_err("Failed to capture stdout")?;
    let stderr = child.stderr.take().wrap_err("Failed to capture stderr")?;
    let stdout_reader = BufReader::new(stdout);
    let stderr_reader = BufReader::new(stderr);
    let stdout_lines_clone = Arc::clone(&stdout_lines);
    let stderr_lines_clone = Arc::clone(&stderr_lines);
    let stdout_thread = thread(stdout_reader, stdout_lines_clone);
    let stderr_thread = thread(stderr_reader, stderr_lines_clone);

    // Wait for the command to complete
    let status = child.wait().wrap_err("Command failed to complete")?;

    // Wait for output reading threads to complete
    stdout_thread.join().unwrap();
    stderr_thread.join().unwrap();

    // Clean up old output
    print!("\r\x1B[K");

    if !status.success() {
        // If there was an error, print the full stderr
        if let Ok(lines) = stderr_lines.lock() {
            println!(
                "\n{}\n{}",
                console::style("Error:").red().bold(),
                lines.join("\n")
            );
        }

        return Err(eyre::eyre!("Failed with exit code: {}", status));
    }

    // If successful, print the full stdout
    if let Ok(lines) = stdout_lines.lock() {
        println!("{}", lines.join("\n"));
    }

    Ok(())
}
