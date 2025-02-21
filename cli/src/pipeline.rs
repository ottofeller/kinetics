use crate::client::Client;
use crate::crat::Crate;
use crate::function::Function;
use eyre::{eyre, Context, Report};
use futures::future;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Debug, Clone)]
pub struct Pipeline {
    is_deploy_enabled: bool,
    is_directly: bool,
    crat: Crate,
    max_concurrent: usize,
}

impl Pipeline {
    pub fn builder() -> PipelineBuilder {
        PipelineBuilder::default()
    }

    pub async fn run(self, functions: Vec<Function>) -> eyre::Result<()> {
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent));
        println!("Building \"{}\"...", &self.crat.name);
        let client = Client::new(&self.is_directly).wrap_err("Failed to create client")?;

        let results = future::try_join_all(functions.into_iter().map(|mut function| {
            let client = client.clone();
            let sem = Arc::clone(&semaphore);
            async move {
                let _permit = sem.acquire().await?;
                let function_name = function.name()?;
                function
                    .build()
                    .await
                    .wrap_err(format!("Failed to build: {}", function_name))?;

                if self.is_deploy_enabled {
                    function.bundle().await?;
                    crate::deploy::upload(&client, &mut function, &self.is_directly)
                        .await
                        .wrap_err(format!("Failed to upload: {}", function.name()?))?;
                }

                Ok::<Function, Report>(function)
            }
        }))
        .await?;

        println!("Build completed: \"{}\"", self.crat.name);

        if self.is_deploy_enabled {
            println!("Deploying \"{}\"...", self.crat.name);
            crate::deploy::deploy(&self.crat, &results, &self.is_directly)
                .await
                .wrap_err("Failed to deploy functions")?;
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct PipelineBuilder {
    is_deploy_enabled: Option<bool>,
    is_directly: Option<bool>,
    crat: Option<Crate>,
    max_concurrent: Option<usize>,
}

impl PipelineBuilder {
    pub fn build(self) -> eyre::Result<Pipeline> {
        if self.crat.is_none() {
            return Err(eyre!("No crate provided to the pipeline"));
        }

        Ok(Pipeline {
            crat: self.crat.unwrap(),
            is_deploy_enabled: self.is_deploy_enabled.unwrap_or(false),
            is_directly: self.is_directly.unwrap_or(false),
            max_concurrent: self.max_concurrent.unwrap_or(4),
        })
    }

    pub fn with_deploy_enabled(mut self, is_deploy_enabled: bool) -> Self {
        self.is_deploy_enabled = Some(is_deploy_enabled);
        self
    }

    pub fn with_directly(mut self, is_directly: bool) -> Self {
        self.is_directly = Some(is_directly);
        self
    }

    pub fn set_crat(mut self, crat: Crate) -> Self {
        self.crat = Some(crat);
        self
    }

    pub fn set_max_concurrent(mut self, max_concurrent: usize) -> Self {
        self.max_concurrent = Some(max_concurrent);
        self
    }
}
