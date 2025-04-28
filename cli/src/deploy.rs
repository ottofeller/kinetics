use crate::function::Function;
use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait DeployConfig: Send + Sync {
    async fn deploy(
        &self,
        toml_string: String,
        secrets: HashMap<String, String>,
        functions: &[Function],
    ) -> eyre::Result<()>;
    async fn upload(&self, function: &mut Function) -> eyre::Result<()>;
}
