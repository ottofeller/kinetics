use eyre::{ContextCompat, Ok, WrapErr};

#[derive(Clone, Debug)]
pub struct Crate {
    pub name: String,
    pub resources: Vec<crate::Resource>,
    pub toml: toml::Value,
}

impl Crate {
    pub fn new(cargo_toml_string: String) -> eyre::Result<Self> {
        let cargo_toml: toml::Value = cargo_toml_string
            .parse::<toml::Value>()
            .wrap_err("Failed to parse Cargo.toml")?;

        Ok(Crate {
            name: cargo_toml
                .get("package")
                .and_then(|pkg| pkg.get("name"))
                .and_then(|name| name.as_str())
                .wrap_err("Failed to get crate name from Cargo.toml")?
                .to_string(),

            resources: Crate::resources(&cargo_toml)?,
            toml: cargo_toml,
        })
    }

    /// All the kinetics related metadata
    pub fn metadata(&self) -> eyre::Result<toml::Value> {
        Ok(self
            .toml
            .clone()
            .get("package")
            .wrap_err("No [package]")?
            .get("metadata")
            .wrap_err("No [metadata]")?
            .get("kinetics")
            .wrap_err("No [kinetics]")?
            .clone())
    }

    /// The hash with all the resources specific to the function
    fn resources(cargo_toml: &toml::Value) -> eyre::Result<Vec<crate::Resource>> {
        let mut result = vec![];

        for category_name in ["kvdb", "queue"] {
            let metadata = cargo_toml
                .get("package")
                .wrap_err("No [package]")?
                .get("metadata");

            // No resources defiend at all
            if metadata.is_none() {
                continue;
            }

            let category = metadata
                .wrap_err("No [metadata]")?
                .get("kinetics")
                .wrap_err("No [kinetics]")?
                .get(category_name);

            // No resources defined in the category
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
                        name: resource
                            .get("name")
                            .unwrap_or(&toml::Value::String(resource_name.clone()))
                            .as_str()
                            .unwrap()
                            .to_string(),
                    }),

                    _ => unreachable!(),
                });
            }
        }

        Ok(result)
    }
}
