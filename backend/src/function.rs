use crate::crat::Crate;
use eyre::{ContextCompat, Ok, WrapErr};

#[derive(Clone, Debug)]
pub struct Function {
    pub id: String,
    pub toml: toml::Value,
    pub s3key: String,

    // Oringal parent crate
    pub crat: Crate,
}

impl Function {
    pub fn new(
        cargo_toml_string: &str,
        crat: &Crate,
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
            mc.decrypt_base64_to_string(&s3key_encrypted)
                .unwrap_or("default".into())
        } else {
            s3key_encrypted.to_string()
        };

        Ok(Function {
            id: uuid::Uuid::new_v4().into(),
            crat: crat.clone(),
            toml: cargo_toml,
            s3key: decrypted,
        })
    }

    pub fn environment(&self) -> eyre::Result<toml::Value> {
        self.toml
            .get("package")
            .wrap_err("No [package]")?
            .get("metadata")
            .wrap_err("No [metadata]")?
            .get("sky")
            .wrap_err("No [sky]")?
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
            .get("sky")
            .wrap_err("No [sky]")?
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

        if meta.get("url_path").is_none() {
            return None;
        }

        Some(meta.get("url_path").unwrap().as_str().unwrap().to_string())
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

    pub fn resources(&self) -> Vec<&crate::Resource> {
        self.crat.resources.iter().clone().collect()
    }
}
