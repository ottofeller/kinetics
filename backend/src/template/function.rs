use crate::template::Crate;
use crate::Resource;
use eyre::{ContextCompat, Ok, WrapErr};

#[derive(Clone, Debug)]
pub struct Function {
    pub id: String,
    pub toml: toml::Value,
    pub s3key: String,
    pub resources: Vec<Resource>,
}

impl Function {
    pub fn new(
        cargo_toml_string: &str,
        s3key_encrypted: &str,
        s3key_decryption_key: &str,
        is_encrypted: bool,
    ) -> eyre::Result<Self> {
        let cargo_toml: toml::Value = cargo_toml_string
            .parse::<toml::Value>()
            .wrap_err("Failed to parse Cargo.toml")?;

        let decrypted = if is_encrypted {
            use magic_crypt::{new_magic_crypt, MagicCryptTrait};
            let mc = new_magic_crypt!(s3key_decryption_key, 256);
            mc.decrypt_base64_to_string(s3key_encrypted)
                .unwrap_or("default".into())
        } else {
            s3key_encrypted.to_string()
        };

        // Load resources from function's Cargo.toml
        let resources = Crate::resources(&cargo_toml)?;

        Ok(Function {
            id: uuid::Uuid::new_v4().into(),
            toml: cargo_toml,
            s3key: decrypted,
            resources,
        })
    }

    pub fn environment(&self) -> eyre::Result<toml::Value> {
        self.toml
            .get("package")
            .wrap_err("No [package]")?
            .get("metadata")
            .wrap_err("No [metadata]")?
            .get("kinetics")
            .wrap_err("No [kinetics]")?
            .get("environment")
            .wrap_err("No [environment]")
            .cloned()
    }

    fn meta(&self) -> eyre::Result<toml::Value> {
        self.toml
            .get("package")
            .wrap_err("No [package]")?
            .get("metadata")
            .wrap_err("No [metadata]")?
            .get("kinetics")
            .wrap_err("No [kinetics]")?
            .get("function")
            .wrap_err("No [function]")
            .cloned()
    }

    /// URL path to be used in the CloudFront distribution
    ///
    /// Optional property, only available for endpoint type of functions.
    pub fn url_path(&self) -> Option<String> {
        if self.meta().is_err() || self.role().unwrap() != "endpoint" {
            return None;
        }

        let meta = self.meta().unwrap();

        Some(meta.get("url_path")?.as_str().unwrap().to_string())
    }

    /// Cron schedule
    ///
    /// Optional property, only available for cron type of functions.
    pub fn schedule(&self) -> Option<String> {
        if self.meta().is_err() || self.role().unwrap() != "cron" {
            return None;
        }

        let meta = self.meta().unwrap();

        Some(meta.get("schedule")?.as_str().unwrap().to_string())
    }

    /// User defined name of the function
    pub fn name(&self) -> eyre::Result<String> {
        Ok(self
            .meta()?
            .get("name")
            .wrap_err("No [name]")?
            .as_str()
            .wrap_err("Not a string")?
            .to_string())
    }

    pub fn role(&self) -> eyre::Result<String> {
        Ok(self
            .meta()?
            .get("role")
            .wrap_err("No [role]")?
            .as_str()
            .wrap_err("Not a string")?
            .to_string())
    }

    /// Returns a list of function's specific resources
    pub(crate) fn resources(&self) -> Vec<&Resource> {
        self.resources.iter().collect()
    }
}
