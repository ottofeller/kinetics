mod docker;
mod local;
mod poller;
mod remote;
mod runner;
mod service;
use crate::{
    runner::{Runnable, Runner},
    writer::Writer,
};
use runner::InvokeRunner;
use std::path::PathBuf;

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

    /// JSON payload for an endpoint or worker.
    ///
    /// For an endpoint, the payload is sent unchanged in the request body.
    /// For a worker, the payload must be a JSON array. Each array item becomes
    /// the body of an individual worker message. If omitted, an empty array is used.
    ///
    /// Example: --payload '[{"name": "John"}]'
    #[arg(short, long, conflicts_with = "payload_file")]
    payload: Option<String>,

    /// Read the JSON payload from a file.
    ///
    /// Uses the same role-specific format as --payload.
    ///
    /// Example: --payload-file payload.json
    #[arg(long, value_name = "PATH")]
    payload_file: Option<PathBuf>,

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

    /// Provision a generic queue.
    /// Helpful when you test a function which sends something to queue.
    /// Not available when called with --remote flag.
    #[arg(long = "with-queue", visible_aliases = ["queue"])]
    with_queue: bool,

    /// Invoke workers locally for the messages the function sends to queues.
    /// Not available when called with --remote flag.
    ///
    /// Pass a comma-separated list of workers:
    /// --with-worker=ExamplesBasicWorkerWorker,ExamplesOtherWorker
    ///
    /// A named queue is provisioned per worker, plus one generic queue.
    /// Messages are routed to the named queue when the target worker has one,
    /// otherwise to the generic queue.
    #[arg(long = "with-worker", visible_aliases = ["worker"], value_name = "WORKERS")]
    with_worker: Option<String>,

    /// Relative path to the project directory
    #[arg(long)]
    pub(crate) project: Option<PathBuf>,
}

impl Runnable for InvokeCommand {
    fn runner(&self, writer: &Writer) -> impl Runner {
        InvokeRunner {
            command: self.clone(),
            writer,
        }
    }
}
