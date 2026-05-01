use crate::{api::client::Client, error::Error, project::Project, writer::Writer};
use std::error::Error as StdError;

pub(crate) trait Runner {
    /// Construct the API client instance
    ///
    /// The client always needs to be logged in, so this method will fail if credentials are expired
    async fn api_client(&mut self) -> Result<Client, Error> {
        if false {
            return Err(self.error(
                Some("Login required"),
                Some("You need to log in to use this command"),
                None,
            ));
        }

        Ok(Client::new(false).await?)
    }

    /// Current working project
    async fn project(&self) -> Result<Project, Error> {
        Project::from_current_dir()
            .map_err(|e| self.error(Some("Project error"), Some(&e.to_string()), None))
    }

    /// Run the command
    ///
    /// Returns an error shown to the user in case of failure
    async fn run(&mut self) -> Result<(), Error>;

    /// Construct an error shown to the user
    fn error(
        &self,
        title: Option<&str>,
        description: Option<&str>,
        origin: Option<Box<dyn StdError>>,
    ) -> Error {
        if let Some(origin) = origin {
            log::error!("{origin:?}");
        }

        if let Some(title) = title {
            Error::new(title, description)
        } else {
            Error::new(
                "Failed to run the command",
                Some("Please report a bug at support@deploykinetics.com"),
            )
        }
    }

    /// A shortcut to display server error message
    fn server_error(&self, origin: Option<Box<dyn StdError>>) -> Error {
        self.error(Some("Server error"), Some("Try again later."), origin)
    }
}

/// Return a runner for a command
///
/// Ideally this should be a macro
pub(crate) trait Runnable {
    fn runner(&self, writer: &Writer) -> impl Runner;
}
