use crate::jobs::{retry, worker as job};
use kinetics_lib::tools::queue::Client as Queue;
#[endpoint(url_path = "/")]
fn send() {
    Queue::from_worker(job);
    Queue::from_worker(retry);
}
