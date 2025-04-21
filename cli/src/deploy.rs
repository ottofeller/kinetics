use crate::function::Function;
use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait DirectDeploy: Send + Sync {
    async fn deploy(
        &self,
        toml_string: String,
        secrets: HashMap<String, String>,
        functions: &[Function],
    ) -> eyre::Result<()>;
    async fn upload(&self, function: &mut Function) -> eyre::Result<()>;
}

/// A default direct deploy plug that fails if used
pub struct DirectDeployPlug {}

#[async_trait]
impl DirectDeploy for DirectDeployPlug {
    async fn deploy(
        &self,
        _toml: String,
        _secrets: HashMap<String, String>,
        _functions: &[Function],
    ) -> eyre::Result<()> {
        unimplemented!()
    }

    async fn upload(&self, _function: &mut Function) -> eyre::Result<()> {
        unimplemented!()
    }
}
