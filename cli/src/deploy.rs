use crate::function::Function;
use async_trait::async_trait;

#[async_trait]
pub trait DirectDeploy: Send + Sync {
    async fn deploy(&self, functions: &[Function]) -> eyre::Result<()>;
    async fn upload(&self) -> eyre::Result<()>;
}

/// A default direct deploy plug that fails if used
pub struct DirectDeployPlug {}

#[async_trait]
impl DirectDeploy for DirectDeployPlug {
    async fn deploy(&self, _functions: &[Function]) -> eyre::Result<()> {
        unimplemented!()
    }

    async fn upload(&self) -> eyre::Result<()> {
        unimplemented!()
    }
}
