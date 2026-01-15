# Kinetics

Kinetics is a hosting platform for Rust applications that allows deploying all types of workloads by writing **only Rust code**.

```rust
#[endpoint(url_path = "/my-rest-endpoint", environment = {"SOME_VAR": "SomeVal"})]
pub async fn endpoint(
    _event: Request<()>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<Response<Body>, BoxError> {
    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body("Hello!".into())?;

    Ok(resp)
}

#[cron(schedule = "rate(1 hour)")]
pub async fn cron(
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<(), BoxError> {
    println!("Started cron job");
    Ok(())
}

#[worker(fifo = true)]
pub async fn worker(
    records: Vec<QueueRecord>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<QueueRetries, BoxError> {
    let mut retries = QueueRetries::new();
    println!("Got records: {records:?}");
    Ok(retries)
}
```

Check out more examples [here](https://github.com/ottofeller/kinetics/tree/main/examples). Including REST API endpoints, queue workers, and cron jobs.

## Features

🦀 **Only Rust code**

Just apply attribute macro to your function, and Kinetics will handle the rest. No other tools required.

🚀 **Any workload**

Deploy REST API endpoints, queue workers, and cron jobs.

🏕️ **Works offline**

Run your functions locally, with no connection to the internet. Requests to DB and queues are served locally as well.

💿 **Comes with DB**

For every project we provision a DB, with connection string being added to function config automatically.

📥 **Comes with queues**

When you launch a worker function, we automatically provision its queue. Other functions access the queue by simply importing the worker function.

🔑 **Secrets**

Automatically provision secrets from `.env.secrets` file or `KINETICS_SECRET_*` environment variables and make them available in your functions.

📚 **Logs**

Monitor your functions with just CLI. Each function gets its own stream of logs.

🤖 **No infrastructure management**

The infrastructure is always provisioned automatically.

## Getting started

```bash
# 1. Install
cargo install kinetics

# 2. Login or sign up, you will receive auth code to this email
kinetics login <email>

# 3. Init a project from template
kinetics init test; cd test

# 4. Call the function locally
kinetics invoke BasicEndpointEndpoint

# 5. Deploy the entire project to the cloud
kinetics deploy

# 6. Alternatively you can deploy only selected functions
kinetics deploy BasicCronCron,BasicWorkerWorker

# 7. Faster way, but you lose rollbacks
kinetics deploy --hotswap BasicCronCron
```

> Kinetics is currently in ⚠️ **active development** and may contain bugs or result in unexpected behavior. The service is free for the first **100,000 invocations** of your functions, regardless of the type of workload.
>
> If you have any issues, please contact us at support@kineticscloud.com.

## Documentation

Kinetics supports several types of workloads(functions): endpoint, worker, and cron.

#### Endpoint

A REST API endpoint. The following attribute macro parameters are available:

- `url_path`: The URL path of the endpoint.
- `environment`: Environment variables.

[Example](https://github.com/ottofeller/kinetics/blob/main/examples/src/environment.rs).

#### Worker

A queue worker. When deployed, a corresponding queue gets provisioned automatically.

- `concurrency`: Max number of concurrent workers.
- `fifo`: Set to true to enable FIFO processing.
- `environment`: Environment variables.

[Example](https://github.com/ottofeller/kinetics/blob/main/examples/src/basic/worker.rs).

#### Cron

A regular job.

- `schedule`: We support [these](https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/aws-resource-scheduler-schedule.html#cfn-scheduler-schedule-scheduleexpression) types of expressions.
- `environment`: Environment variables.

[Example](https://github.com/ottofeller/kinetics/blob/main/examples/src/basic/cron.rs).

#### Env vars

A macro for any type of workload accepts JSON array with environment variables.

[Example](https://github.com/ottofeller/kinetics/blob/8cab4e6719b7dea944459ca59a82935d5e30e074/examples/src/environment.rs).

#### Secrets

Store secrets in `.env.secrets` file in the root directory of your crate. Kinetics will automatically pick it up and provision to all of your workloads in the second parameter of the function as `HashMap<String, String>`.

Alternatively store the secrets in environment variables starting with `KINETICS_SECRET_`. This way might be more suitable for CI/CD environments.

[Example](https://github.com/ottofeller/kinetics/blob/main/examples/src/secrets.rs).

#### Database

We automatically provision one SQL DB for each project. Also `kinetics invoke --with-db [Function name]` will automatically provision DB locally and replace the connection string with local endpoint.

You can then interact with it like you normally interact with a SQL DB, [example](https://github.com/ottofeller/kinetics/blob/main/examples/src/database.rs).

### Examples

Try in `examples/` dir. These are the most frequently used commands with examples of input params.

Print out all available functions before deployment. Their names and URLs of REST API endpoints:

```sh
kinetics func list
```

Invoke a function locally with parameters. `--payload` sets the JSON body payload for endpoint and worker functions:

```sh
kinetics invoke BasicWorkerWorker --payload '{"name": "John"}'
```

Invoke endpoint with http headers:

```sh
kinetics invoke BasicWorkerWorker --headers '{"Authorization": "Bearer 123"}'
```

Invoke a function which needs a DB. DB gets provisioned locally and is fully operational, not just a mock for requests.

```sh
kinetics invoke DatabaseDatabase --with-db
```

Invoke a function with a DB and applied migrations. The migrations are pulled from `migrations` folder within the project by default, or one can provide a custom path.

```sh
kinetics invoke DatabaseDatabase --with-db --with-migrations my-migrations
```

Deploy entire project:

```sh
kinetics deploy
```

Deploy individual functions:

```sh
kinetics deploy DatabaseDatabase,BasicWorkerWorker
```

Deploy faster, but without the ability to roll back.

```sh
kinetics deploy --hotswap DatabaseDatabase
```

Invoke a function remotely by automatically resolving function's name into the URL:

```sh
kinetics invoke DatabaseDatabase --remote
```

Create a DB migration file:

```sh
kinetics migrations create init
# Creates ./migrations/20251124125730_init.up.sql
```

Apply DB migrations from a local folder to the production DB:

```sh
kinetics migrations apply
```

Output logs for a function:

```sh
kinetics func logs BasicEndpointEndpoint
```

Output run statistics for a function:

```sh
kinetics func stats BasicEndpointEndpoint
```

## CI/CD
### Initializing
A GitHub workflow is automatically created in projects initialized with `kientics init`. If you need to add GitHub workflow in existing project do the following in the dir of the project:
```sh
kinetic cicd init
```

### Access token
To make GitHub workflow work you need to provide it with kinetics access token:
- After calling `kinetics init <project-name>`
- In your terminal run `kinetics auth token` to get a token (you need to be logged in)
- Add it as `KINETICS_ACCESS_TOKEN` secret to your repo to enable deploys (check example below).

This workflow enables automatic cloud deployment of any update in the main branch.

### Secrets
In order to provide your functions with secrets residing in `.env.secrets` you need to add them to the `env` section of the `Run kinetics deploy` step with `KINETICS_SECRET_` prefix, e.g.:

```yaml
- name: Run kinetics deploy
  env:
    DEPLOY_DIR: .
    KINETICS_ACCESS_TOKEN: ${{ secrets.KINETICS_ACCESS_TOKEN }}
    KINETICS_SECRET_MY_SECRET: ${{ secrets.MY_SECRET }}
```

## Support & Community

- support@usekinetics.com. Help with builds, deployments, and runtime.
- [GitHub Issues](https://github.com/ottofeller/kinetics/issues). Persistent bugs, and feature requests.
