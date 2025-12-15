use eyre::{Context, ContextCompat};
use std::process::{Child, ExitStatus};
use std::{
    io::{BufRead, BufReader, Read, Write},
    sync::{Arc, Mutex},
};

/// A wrapper over system process
///
/// It is used to implement various helpers and utilities.
pub struct Process {
    child: Child,

    // Collect all lines for later display
    stdout_lines: Arc<Mutex<Vec<String>>>,
    stderr_lines: Arc<Mutex<Vec<String>>>,
}

impl Process {
    pub fn new(child: Child) -> Self {
        Process {
            child,
            stdout_lines: Arc::new(Mutex::new(Vec::new())),
            stderr_lines: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Create a thread for printing out a line, and accumulating for later full output
    fn thread(
        &self,
        reader: BufReader<impl Read + Send + 'static>,
        lock: Arc<Mutex<Vec<String>>>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            for line in reader.lines().map_while(Result::ok) {
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
        })
    }

    /// Print out process output in one line, in real-time
    pub fn log(&mut self) -> eyre::Result<ExitStatus> {
        // Create readers for stdout and stderr
        let stdout = self
            .child
            .stdout
            .take()
            .wrap_err("Failed to capture stdout")?;

        let stderr = self
            .child
            .stderr
            .take()
            .wrap_err("Failed to capture stderr")?;

        let stdout_reader = BufReader::new(stdout);
        let stderr_reader = BufReader::new(stderr);
        let stdout_lines_clone = Arc::clone(&self.stdout_lines);
        let stderr_lines_clone = Arc::clone(&self.stderr_lines);
        let stdout_thread = self.thread(stdout_reader, stdout_lines_clone);
        let stderr_thread = self.thread(stderr_reader, stderr_lines_clone);

        // Wait for the command to complete
        let status = self.child.wait().wrap_err("Command failed to complete")?;

        // Wait for output reading threads to complete
        stdout_thread.join().unwrap();
        stderr_thread.join().unwrap();

        // Clean up old output
        print!("\r\x1B[K");

        Ok(status)
    }

    /// If there was an error, print the full stderr
    pub fn print_error(&self) {
        if let Ok(lines) = self.stderr_lines.lock() {
            println!(
                "\n{}\n{}",
                console::style("Error:").red().bold(),
                lines.join("\n")
            );
        }
    }

    /// Print the full output
    pub fn print(&self) {
        if let Ok(lines) = self.stdout_lines.lock() {
            println!("{}", lines.join("\n"));
        }
    }
}
