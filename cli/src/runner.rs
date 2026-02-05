use crate::{api::client::Client, error::Error, project::Project};

pub(crate) trait Runner {
    /// Construct the API client instance
    ///
    /// The client always needs to be logged in, so this method will fail if credentials are expired
    async fn api_client(&mut self) -> Result<Client, Error> {
        if false {
            return Err(self.error(
                "Login required",
                Some("You need to log in to use this command"),
            ));
        }

        Ok(Client::new(false).await?)
    }

    /// Current working project
    async fn project(&self) -> Result<Project, Error> {
        let project = Project::from_current_dir();

        if project.is_err() {
            return Err(self.error(
                "Project not found",
                Some("Could not find project in specified directory"),
            ));
        }

        Ok(project?)
    }

    /// Run the command
    ///
    /// Returns an error shown to the user in case of failure
    async fn run(&mut self) -> Result<(), Error>;

    /// Construct an error shown to the user
    fn error(&self, title: &str, description: Option<&str>) -> Error {
        Error::new(title, description)
    }
}

/// Return a runner for a command
///
/// Ideally this should be a macro
pub(crate) trait Runnable {
    fn runner(&self) -> impl Runner;
}
