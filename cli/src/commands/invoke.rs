mod docker;
mod local;
mod remote;
mod runner;
mod service;
use crate::runner::{Runnable, Runner};
use runner::InvokeRunner;

#[derive(clap::Args, Clone)]
pub(crate) struct InvokeCommand {
    /// Name of a function, use "kinetics func list" to see all names
    #[arg()]
    name: String,

    /// Headers to be sent to endpoint function, in JSON.
    ///
    /// Example: --headers '{"auth": "Bearer 111"}'.
    #[arg(long)]
    headers: Option<String>,

    /// Set URL path while calling endpoint function.
    /// Required for endpoints with parametrized URLs, e.g. /user/*/profile.
    ///
    /// Example: --url-path /user/1/profile
    #[arg(long)]
    url_path: Option<String>,

    /// Must be a valid JSON.
    ///
    /// In case of endpoint functions payload is a body.
    /// In case of workers, payload is a single event of a queue, which will be wrapped in array and passed to worker function.
    ///
    /// Example: --payload '{"name": "John Smith"}'
    #[arg(short, long)]
    payload: Option<String>,

    /// Invoke function remotely. Only works if function was deployed before.
    #[arg(short, long)]
    remote: bool,

    /// [DEPRECATED]
    #[arg(short, long)]
    table: Option<String>,

    /// Provision local SQL database for invoked function to use. Not available when called with --remote flag.
    #[arg(long="with-database", visible_aliases=["with-db", "db"])]
    with_database: bool,

    /// Apply migrations to locally provisioned database. Not available when called with --remote flag.
    ///
    /// Accepts a path to dir with SQL-files relative to crate's root, defaults to <crate>/migrations/
    #[arg(short, long = "with-migrations", num_args = 0..=1, default_missing_value = "")]
    with_migrations: Option<String>,

    /// Provision a queue. Helpful when you test a function which sends something to queue. Not available when called with --remote flag.
    #[arg(long="with-queue", visible_aliases=["queue"])]
    with_queue: bool,
}

impl Runnable for InvokeCommand {
    fn runner(&self) -> impl Runner {
        InvokeRunner {
            command: self.clone(),
        }
    }
}
