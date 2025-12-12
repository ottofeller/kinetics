use crate::{function::Function, project::Project};
use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait DeployConfig: Send + Sync {
    async fn deploy(
        &self,
        project: &Project,
        secrets: HashMap<String, String>,
        functions: &[Function],
    ) -> eyre::Result<bool>;
    async fn upload(&self, function: &mut Function) -> eyre::Result<bool>;
}
