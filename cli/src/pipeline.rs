use crate::client::Client;
use crate::crat::Crate;
use crate::function::Function;
use eyre::{eyre, Context, Report};
use futures::future;
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct Pipeline {
    functions: Vec<Function>,
    is_deploy_enabled: bool,
    is_directly: bool,
    crat: Crate,
    max_concurrent: usize,
}

impl Pipeline {
    pub fn builder() -> PipelineBuilder {
        PipelineBuilder::default()
    }

    pub async fn run(self) -> eyre::Result<()> {
        println!("Building \"{}\"...", self.crat.name);
        let client = Client::new(&self.is_directly).wrap_err("Failed to create client")?;

        // Define maximum number of parallel requests
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent));

        let handles = self.functions.clone().into_iter().map(|mut function| {
            let client = client.clone();
            let sem = Arc::clone(&semaphore);

            tokio::spawn(async move {
                // Acquire permit before sending request.
                let _permit = sem.acquire().await?;

                let function_name = function.name()?;

                function
                    .build()
                    .await
                    .wrap_err(format!("Failed to build: {}", function_name))?;

                if !self.is_deploy_enabled {
                    return Ok(());
                }

                function.bundle().await?;

                crate::deploy::upload(&client, &mut function, &self.is_directly)
                    .await
                    .wrap_err(format!("Failed to upload: {}", function.name()?))?;

                Ok::<(), Report>(())
            })
        });

        let errors: Vec<_> = future::join_all(handles)
            .await
            .into_iter()
            .map(|res| {
                res.map_err(Report::msg)
                    .and_then(|inner_result| inner_result)
            })
            .filter_map(|res| res.err())
            .collect();

        if !errors.is_empty() {
            println!("Failed to deploy functions:");
            for error in errors {
                println!("{}", error);
            }
            return Err(eyre!("Failed to deploy functions"));
        }

        println!("Build completed: \"{}\"", self.crat.name);

        if !self.is_deploy_enabled {
            return Ok(());
        }

        println!("Deploying \"{}\"...", self.crat.name);

        crate::deploy::deploy(&self.crat, &self.functions, &self.is_directly)
            .await
            .wrap_err("Failed to deploy functions")?;

        Ok(())
    }
}

#[derive(Default)]
pub struct PipelineBuilder {
    is_deploy_enabled: Option<bool>,
    is_directly: Option<bool>,
    functions: Vec<Function>,
    crat: Option<Crate>,
    max_concurrent: Option<usize>,
}

impl PipelineBuilder {
    pub fn build(self) -> eyre::Result<Pipeline> {
        if self.functions.is_empty() {
            return Err(eyre!("No functions provided to the pipeline"));
        }

        if self.crat.is_none() {
            return Err(eyre!("No crate provided to the pipeline"));
        }

        Ok(Pipeline {
            crat: self.crat.unwrap(),
            functions: self.functions,
            is_deploy_enabled: self.is_deploy_enabled.unwrap_or(false),
            is_directly: self.is_directly.unwrap_or(false),
            max_concurrent: self.max_concurrent.unwrap_or(2),
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

    pub fn set_functions(mut self, functions: Vec<Function>) -> Self {
        self.functions.extend(functions);
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
