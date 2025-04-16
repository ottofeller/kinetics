use crate::client::Client;
use crate::crat::Crate;
use crate::function::Function;
use crate::secret::Secret;
use color_eyre::owo_colors::OwoColorize;
use common::auth::lambda;
use eyre::{ContextCompat, WrapErr};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

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

                // Trim the line to 48 characters with ellipsis if necessary
                let line_trimmed = if line.trim().len() > 48 {
                    format!("{}...", line.trim().chars().take(45).collect::<String>())
                } else {
                    line.trim().to_string()
                };

                print!("{}", console::style(&line_trimmed.trim()).dim());
                let _ = std::io::stdout().flush();
            }
        }
    })
}

/// Invoke the function locally
pub async fn invoke(
    function: &Function,
    crat: &Crate,
    payload: &str,
    headers: &str,
) -> eyre::Result<()> {
    let home = std::env::var("HOME").wrap_err("Can not read HOME env var")?;

    // Load secrets from .env.secrets if it exists
    let mut secrets = HashMap::new();

    for secret in Secret::from_dotenv().wrap_err("Failed to read secrets")? {
        secrets.insert(
            format!("KINETICS_SECRET_{}", secret.name.clone()),
            secret.value(),
        );
    }

    let client = Client::new().wrap_err("Failed to create client")?;

    let credentials: lambda::JsonResponse = client
        .post("/auth/lambda")
        .json(&serde_json::json!(lambda::JsonBody {
            crate_name: crat.name.clone(),
            function_name: function.name()?.clone(),
        }))
        .send()
        .await
        .wrap_err("Failed to get auth credentials")?
        .json()
        .await
        .wrap_err("Invalid response")?;

    let invoke_dir =
        Path::new(&home).join(format!(".kinetics/{}/{}Local", crat.name, function.name()?));

    let display_path = format!("{}", invoke_dir.display());

    println!(
        "\n{} {} {}",
        console::style("Invoking function").green().bold(),
        console::style("from").dimmed(),
        console::style(&display_path).underlined().bold()
    );

    // Collect all lines for later display
    let stdout_lines = Arc::new(Mutex::new(Vec::new()));
    let stderr_lines = Arc::new(Mutex::new(Vec::new()));

    // Start the command with piped stdout and stderr
    let mut child = Command::new("cargo")
        .args(["run"])
        .envs(secrets)
        .env("AWS_ACCESS_KEY_ID", credentials.access_key_id)
        .env("AWS_SECRET_ACCESS_KEY", credentials.secret_access_key)
        .env("AWS_SESSION_TOKEN", credentials.session_token)
        .env("KINETICS_INVOKE_PAYLOAD", payload)
        .env("KINETICS_INVOKE_HEADERS", headers)
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
