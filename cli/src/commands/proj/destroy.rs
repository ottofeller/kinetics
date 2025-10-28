use crate::crat::Crate;
use crate::project::Project;
use crossterm::style::Stylize;
use eyre::{eyre, Context, Result};
use std::io::{self, Write};

pub async fn destroy(crat: &Option<Crate>, name: Option<&str>) -> Result<()> {
    if crat.is_none() && name.is_none() {
        return Err(eyre!(
            "Either provide --name argument or run command in project's dir"
        ));
    }

    let project_name = match name {
        Some(name) => name,
        None => crat.as_ref().unwrap().name.as_str(),
    };

    let project = match Project::one(project_name).await {
        Ok(project) => project,
        Err(_) => {
            println!("{}", "Project not found".yellow());
            return Ok(());
        }
    };

    print!("{} {}: ", "Do you want to proceed?".bold(), "[y/N]".dim());
    io::stdout().flush()?;
    let mut input = String::new();

    io::stdin()
        .read_line(&mut input)
        .wrap_err("Failed to read input")?;

    if !matches!(input.trim().to_lowercase().as_ref(), "y" | "yes") {
        println!("{}", "Destroying canceled".dim().bold());
        return Ok(());
    }

    println!("{}: {}", "Destroying".bold(), &project.name);
    project.destroy().await?;
    println!("{}", console::style("Project destroyed").green());
    Ok(())
}
