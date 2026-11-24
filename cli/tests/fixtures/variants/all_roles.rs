#[worker(dlq = retry)]
fn worker() {}

#[dlq_worker]
fn retry() {}

#[cron(schedule = "rate(1 minute)")]
fn cron() {}
