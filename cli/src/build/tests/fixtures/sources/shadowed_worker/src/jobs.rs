use kinetics_lib::tools::queue::Client;
#[worker]
fn worker() {
    let worker = || ();
    Client::from_worker(worker);
}
