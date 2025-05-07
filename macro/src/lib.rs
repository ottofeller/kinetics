use kinetics_parser::{Cron, Endpoint, Worker};
use proc_macro::TokenStream;
use syn::parse_macro_input;

/// Lambda endpoint
///
/// Parameters:
/// - `name`: override the function name
/// - `url_path`: URL path of the endpoint
/// - `environment`: environment variables
/// - `queues`: SQS queues accessible from the lambda
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
/// - `queue_alias`: alias of the queue processed by the worker
/// - `concurrency`: max number of concurrent workers
/// - `fifo`: set to true to enable FIFO processing
/// - `environment`: environment variables
#[proc_macro_attribute]
pub fn worker(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the macro attributes in order to validate the inputs,
    // then discard the result.s
    let _args = parse_macro_input!(attr as Worker);
    item
}
