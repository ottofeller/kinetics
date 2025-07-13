use crate::build::pipeline::Pipeline;
use crate::crat::Crate;
use crate::function::Function;
use async_trait::async_trait;
use eyre::{Ok, WrapErr};
use std::collections::HashMap;
use std::sync::Arc;

/// The entry point to run the command
pub async fn run(
    all_functions: &[Function],
    deploy_functions: &[String],
    max_concurrency: &usize,
    deploy_config: Option<Arc<dyn DeployConfig>>,
) -> eyre::Result<()> {
    Pipeline::builder()
        .set_max_concurrent(*max_concurrency)
        .with_deploy_enabled(true)
        .with_deploy_config(deploy_config)
        .set_crat(Crate::from_current_dir()?)
        .build()
        .wrap_err("Failed to build pipeline")?
        .run(all_functions, deploy_functions)
        .await?;

    Ok(())
}

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
