use crate::client::Client;
use crate::crat::Crate;
use eyre::{Context, Result};
use serde_json::json;
use std::io::{self, Write};

pub async fn destroy(crat: &Crate) -> Result<()> {
    let client = Client::new(false).await.wrap_err("Failed to create client")?;

    print!(
        "{} {}: ",
        console::style("Do you want to proceed?").bold(),
        console::style("[y/N]").dim()
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .wrap_err("Failed to read input")?;

    let confirmed = matches!(input.trim().to_lowercase().as_ref(), "y" | "yes");

    if !confirmed {
        println!("{}", console::style("Destroying canceled").dim().bold());
        return Ok(());
    }

    println!(
        "{}: {}",
        console::style("Destroying").bold(),
        console::style(&crat.name)
    );

    client
        .post("/stack/destroy")
        .json(&json!({"crate_name": crat.name}))
        .send()
        .await?;

    println!("{}", console::style("Application destroyed").green());
    Ok(())
}
