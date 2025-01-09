use eyre::{ContextCompat, Ok, WrapErr};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Crate {
    pub name: String,
    pub resources: Vec<crate::Resource>,
    pub toml: toml::Value,
}

impl Crate {
    pub fn new(path: PathBuf) -> eyre::Result<Self> {
        let cargo_toml: toml::Value = std::fs::read_to_string(path.join("Cargo.toml"))
            .wrap_err("Failed to read Cargo.toml: {cargo_toml_path:?}")?
            .parse::<toml::Value>()
            .wrap_err("Failed to parse Cargo.toml")?;

        Ok(Crate {
            name: cargo_toml
                .get("package")
                .and_then(|pkg| pkg.get("name"))
                .and_then(|name| name.as_str())
                .wrap_err("Failed to get crate name from Cargo.toml")?
                .to_string(),

            resources: Crate::resources(&path)?,
            toml: cargo_toml,
        })
    }

    /// The hash with all the resources specific to the function
    fn resources(path: &PathBuf) -> eyre::Result<Vec<crate::Resource>> {
        let mut result = vec![];
        let src_path = Path::new(path);
        let cargo_toml_path = src_path.join("Cargo.toml");

        let cargo_toml: toml::Value = std::fs::read_to_string(cargo_toml_path)
            .wrap_err("Failed to read Cargo.toml: {cargo_toml_path:?}")?
            .parse::<toml::Value>()
            .wrap_err("Failed to parse Cargo.toml")?;

        for category_name in vec!["kvdb", "queue"] {
            let category = cargo_toml
                .get("package")
                .wrap_err("No [package]")?
                .get("metadata")
                .wrap_err("No [metadata]")?
                .get("sky")
                .wrap_err("No [sky]")?
                .get(category_name);

            if category.is_none() {
                continue;
            }

            let category = category
                .wrap_err(format!("No category {category_name} found"))?
                .as_table()
                .wrap_err("Section format is wrong")?;

            for resource_name in category.keys() {
                let resource = category
                    .get(resource_name)
                    .wrap_err("No {resource_name}")?
                    .clone();

                result.push(match category_name {
                    "queue" => crate::Resource::Queue(crate::Queue {
                        name: resource_name.clone(),

                        concurrency: resource
                            .get("concurrency")
                            .unwrap_or(&toml::Value::Integer(1))
                            .as_integer()
                            .unwrap() as u32,
                    }),

                    "kvdb" => crate::Resource::KvDb(crate::KvDb {
                        name: resource_name.clone(),
                    }),

                    _ => unreachable!(),
                });
            }
        }

        Ok(result)
    }
}
