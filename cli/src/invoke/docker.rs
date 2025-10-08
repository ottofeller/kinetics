use crate::error::Error;
use crate::invoke::{dynamodb::LocalDynamoDB, sqldb::LocalSqlDB};
use crate::process::Process;
use eyre::{eyre, Context};
use serde_yaml::{Mapping, Value};
use std::{path::Path, path::PathBuf, process, process::Stdio};

pub enum DockerService {
    DynamoDB(LocalDynamoDB),
    SqlDB(LocalSqlDB),
}

pub struct Docker {
    /// Path to .kinetics dir
    build_path: PathBuf,

    /// A flag indicating the instance was started
    is_started: bool,

    /// List of services to start
    services: Vec<DockerService>,
}

impl Docker {
    pub fn new(build_path: &Path) -> Self {
        Self {
            build_path: build_path.to_owned(),
            is_started: false,
            services: vec![],
        }
    }

    /// Start docker containers
    pub fn start(&mut self) -> eyre::Result<()> {
        // There is nothing to start if there are no services
        if self.services.is_empty() {
            return Ok(());
        }

        let docker_compose_file = self.docker_compose_string()?;
        let dest = self.docker_compose_path();

        std::fs::write(&dest, docker_compose_file)
            .inspect_err(|e| {
                log::error!("Failed to write DOCKER_COMPOSE_FILE to {:?}: {}", dest, e)
            })
            .wrap_err(Error::new(
                "Failed to set up Docker",
                Some(&format!("Make sure you can write to {dest:?}")),
            ))?;

        // Config file functionality must ensure that the root dirs are all valid
        let file_path = dest.to_string_lossy();

        let child = process::Command::new("docker-compose")
            .args(["-f", &file_path, "up", "-d"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .wrap_err("Failed to execute docker-compose")?;

        let mut process = Process::new(child);
        let status = process.log()?;

        if !status.success() {
            process.print_error();

            return Err(Error::new(
                "Failed to start Docker containers",
                Some("Make sure the docker is installed and running."),
            )
            .into());
        }

        self.is_started = true;

        Ok(())
    }

    /// Stop DynamoDB container
    pub fn stop(&self) -> eyre::Result<()> {
        if !self.is_started {
            // self.start was not called
            return Ok(());
        }

        let status = process::Command::new("docker-compose")
            .args(["-f", &self.docker_compose_path().to_string_lossy(), "down"])
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .status()
            .inspect_err(|e| log::error!("Error: {}", e))
            .wrap_err("Failed to execute docker-compose")?;

        if !status.success() {
            return Err(eyre::eyre!(
                "docker-compose command failed with exit code: {}",
                status
            ));
        }

        Ok(())
    }

    /// Provision each service individually
    pub async fn provision(&self) -> eyre::Result<()> {
        // Collect futures (only for services that actually need provisioning)
        let tasks = self
            .services
            .iter()
            .filter_map(|service| match service {
                DockerService::DynamoDB(svc) => Some(svc.provision()),
                DockerService::SqlDB(_) => None, // no-op
            })
            .collect::<Vec<_>>();

        // Run all futures in parallel
        futures::future::try_join_all(tasks)
            .await
            .wrap_err("Failed to provision services")?;

        Ok(())
    }

    pub fn with_dynamodb(&mut self, dynamodb: LocalDynamoDB) {
        self.services.push(DockerService::DynamoDB(dynamodb));
    }

    pub fn with_sqldb(&mut self, sqldb: LocalSqlDB) {
        self.services.push(DockerService::SqlDB(sqldb));
    }

    /// Creates docker-compose.yml string with all docker services
    fn docker_compose_string(&self) -> eyre::Result<String> {
        // Contains all services for docker-compose.yml file
        let mut services = Mapping::new();

        for service in &self.services {
            // Prepare service YAML snippets for each service
            let service_snippet = match service {
                DockerService::DynamoDB(service) => service.docker_compose_snippet(),
                DockerService::SqlDB(service) => service.docker_compose_snippet(),
            };

            let value: Value = serde_yaml::from_str(service_snippet)
                .wrap_err("failed to parse service YAML snippet")?;

            let mapping = value
                .as_mapping()
                .ok_or_else(|| eyre!("Failed to parse service YAML snippet"))?;

            for (k, v) in mapping {
                services.insert(k.clone(), v.clone());
            }
        }

        // Create root YAML for docker-compose.yml file and insert services
        let mut root = Mapping::new();

        root.insert(
            Value::String("services".to_string()),
            Value::Mapping(services),
        );

        let docker_compose = serde_yaml::to_string(&Value::Mapping(root))
            .wrap_err("failed to serialize docker-compose YAML")?;

        Ok(docker_compose)
    }

    /// Path to docker-compose.yml file
    fn docker_compose_path(&self) -> PathBuf {
        self.build_path.join("docker-compose.yml")
    }
}

impl Drop for Docker {
    fn drop(&mut self) {
        self.stop().unwrap();
    }
}
