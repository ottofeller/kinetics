use eyre::{eyre, ContextCompat, Ok, WrapErr};

#[derive(Clone, Debug)]
pub struct Crate {
    pub name: String,
    pub name_escaped: String,
    pub resources: Vec<crate::Resource>,
    pub toml: toml::Value,
}

impl Crate {
    pub fn new(cargo_toml_string: &str) -> eyre::Result<Self> {
        let cargo_toml: toml::Value = cargo_toml_string
            .parse::<toml::Value>()
            .wrap_err("Failed to parse Cargo.toml")?;

        let name = cargo_toml
            .get("package")
            .and_then(|pkg| pkg.get("name"))
            .and_then(|name| name.as_str())
            .wrap_err("Failed to get crate name from Cargo.toml")?
            .to_string();

        let name_escaped = name.replace("-", "HYPHEN").replace("_", "UNDRSC");
        if !name_escaped.chars().all(char::is_alphanumeric) {
            return Err(eyre!("Invalid crate name"));
        }

        Ok(Crate {
            name,
            name_escaped,
            resources: Crate::resources(&cargo_toml)?,
            toml: cargo_toml,
        })
    }

    /// All the kinetics related metadata
    pub fn metadata(&self) -> eyre::Result<toml::Value> {
        Ok(self
            .toml
            .get("package")
            .wrap_err("No [package]")?
            .get("metadata")
            .wrap_err("No [metadata]")?
            .get("kinetics")
            .wrap_err("No [kinetics]")?
            .clone())
    }

    /// The hash with all the resources specific to the function
    pub(crate) fn resources(cargo_toml: &toml::Value) -> eyre::Result<Vec<crate::Resource>> {
        let mut result = vec![];

        for category_name in ["kvdb", "queue"] {
            let Some(metadata) = cargo_toml
                .get("package")
                .wrap_err("No [package]")?
                .get("metadata")
            else {
                // No resources defined at all
                continue;
            };

            let Some(category) = metadata
                .get("kinetics")
                .wrap_err("No [kinetics]")?
                .get(category_name)
            else {
                // No resources defined in the category
                continue;
            };

            let category = category.as_table().wrap_err("Section format is wrong")?;

            for resource_name in category.keys() {
                let resource = category
                    .get(resource_name)
                    .wrap_err("No {resource_name}")?
                    .clone();

                result.push(match category_name {
                    "queue" => crate::Resource::Queue(crate::Queue {
                        alias: resource
                            .get("alias")
                            .unwrap_or(&toml::Value::String(resource_name.clone()))
                            .as_str()
                            .unwrap_or("")
                            .to_string(),

                        name: resource_name.clone(),

                        // Defined later
                        cfn_name: None,

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
