use crate::error::Error;
use crate::project::Project;
use eyre::Context as _;
use std::fs;
use std::io::Write as _;
use toml_edit::{value, DocumentMut};

pub(super) fn save_domain(project: &Project, domain: &str) -> eyre::Result<()> {
    let mut doc = read_config(project)?;
    doc["domain"] = value(domain);
    write_config(project, &doc)
}

pub(super) fn remove_domain(project: &Project) -> eyre::Result<()> {
    let mut doc = read_config(project)?;
    doc.remove("domain");
    write_config(project, &doc)
}

fn read_config(project: &Project) -> eyre::Result<DocumentMut> {
    let config_path = project.path.join("kinetics.toml");

    let config_content = match fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            format!("[project]\nname = \"{}\"\n", project.name)
        }
        Err(error) => {
            return Err(error).wrap_err(Error::new(
                &format!("Failed to read {}", config_path.display()),
                None,
            ));
        }
    };

    config_content
        .parse::<DocumentMut>()
        .wrap_err("Failed to parse kinetics.toml")
}

fn write_config(project: &Project, doc: &DocumentMut) -> eyre::Result<()> {
    let config_path = project.path.join("kinetics.toml");
    let config_path_str = config_path.display().to_string();

    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(config_path)
        .wrap_err(Error::new(
            &format!("Failed to open {}", &config_path_str),
            None,
        ))?;

    file.write_all(doc.to_string().as_bytes())
        .wrap_err(Error::new(
            &format!("Failed to write to {}", &config_path_str),
            None,
        ))?;

    Ok(())
}
