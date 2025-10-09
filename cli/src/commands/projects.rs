use crate::crat::Crate;
use crate::error::Error;
use crate::project::Project;
use eyre::ContextCompat;
use futures::stream::{self, StreamExt};
use tabled::settings::{peaker::Priority, style::Style, Settings, Width};
use tabled::{Table, Tabled};
use terminal_size::{terminal_size, Width as TerminalWidth};

/// Prints out the list of all projects
pub async fn list() -> Result<(), Error> {
    #[derive(Tabled, Clone)]
    struct ProjectRow {
        #[tabled(rename = "Name")]
        name: String,
        #[tabled(rename = "Status")]
        status: String,
    }

    let projects = stream::iter(Project::all().await?)
        .map(async |project| {
            Crate::status_by_name(&project.name)
                .await
                .map(|status| ProjectRow {
                    name: project.name,
                    status: status.status,
                })
        })
        .buffer_unordered(3) // Run up to 3 requests concurrently.
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

    // Check terminal size and setup output table accordingly
    let (TerminalWidth(width), _) = terminal_size().wrap_err("failed to obtain a terminal size")?;
    let width: usize = width.into();
    let settings = Settings::default()
        .with(Width::wrap(width).priority(Priority::max(true)))
        .with(Width::increase(width));

    let mut table = Table::new(projects);
    table.with(Style::modern()).with(settings);
    println!("Projects\n{}", table);

    Ok(())
}
