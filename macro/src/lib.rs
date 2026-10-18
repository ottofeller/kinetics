use kinetics_parser::{Cron, DlqWorker, Endpoint, Worker};
use proc_macro::TokenStream;
use syn::parse_macro_input;

/// Lambda endpoint
///
/// Parameters:
/// - `name`: override the function name
/// - `url_path`: URL path of the endpoint
/// - `environment`: environment variables
#[proc_macro_attribute]
pub fn endpoint(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the macro attributes in order to validate the inputs,
    // then discard the result.
    let _args = parse_macro_input!(attr as Endpoint);
    item
}

/// Cron lambda
///
/// Parameters:
/// - `name`: override the function name
/// - `schedule`: [Schedule expression](https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/aws-resource-scheduler-schedule.html#cfn-scheduler-schedule-scheduleexpression)
/// - `environment`: environment variables
#[proc_macro_attribute]
pub fn cron(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the macro attributes in order to validate the inputs,
    // then discard the result.
    let _args = parse_macro_input!(attr as Cron);
    item
}

/// Worker lambda
///
/// Parameters:
/// - `name`: override the function name
/// - `concurrency`: max number of concurrent workers
/// - `fifo`: set to true to enable FIFO processing
/// - `batch_size`: max number of records to process in a single batch (1..10) for fifo queues
///   and (1..100) for standard queues
/// - `dlq`: name of the `#[dlq_worker]` consuming this worker's DLQ; the function
///   must be defined in the same file
/// - `max_receive_count`: attempts before the main queue redrives a record to the DLQ
///   (SQS `RedrivePolicy`, default 3)
/// - `environment`: environment variables
#[proc_macro_attribute]
pub fn worker(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the macro attributes in order to validate the inputs,
    // then discard the result.s
    let _args = parse_macro_input!(attr as Worker);
    item
}

/// DLQ recovery worker lambda
///
/// Parameters:
/// - `concurrency`: max number of concurrent workers (default 2)
/// - `batch_size`: max number of records to process in a single batch (1..10) for fifo
///   queues and (1..100) for standard queues; FIFO is inherited from the worker
/// - `environment`: environment variables
#[proc_macro_attribute]
pub fn dlq_worker(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the macro attributes in order to validate the inputs,
    // then discard the result.
    let _args = parse_macro_input!(attr as DlqWorker);
    item
}
