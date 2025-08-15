# Kinetics

Kinetics is a hosting platform for Rust applications that allows deploying all types of workloads by writing **only Rust code**.

```rust
#[endpoint(
    url_path = "/path",
    environment = {"SOME_VAR": "SomeVal"},
)]
pub async fn endpoint(
    _event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body("Hello!".into())?;

    Ok(resp)
}
```

Check out more examples [here](https://github.com/ottofeller/kinetics/tree/main/examples). Including REST API endpoints, queue workers, and cron jobs.


## Features

🦀 **Only Rust code**

Just apply attribute macro to your function, and Kinetics will handle the rest. No other tools required.

🚀 **Any workload**

Deploy REST API endpoints, queue workers, and cron jobs.

🏕️ **Works offline**

Test your functions locally with no connection to the internet. We mock DB as well, so all requests to DB also work locally. No configuration required.

💿 **Comes with DB**

Seamlessly provision KV DB if your workload needs a persistent storage.

🔑 **Secrets**

Automatically provision secrets from `.env.secrets` file.

📚 **Logs**
Monitor your functions with just CLI.

🤖 **No infrastructure management**

The infrastructure is provisioned automatically, e.g. a queue for the worker workload.

## Getting started

```bash
# 1. Install
cargo install kinetics

# 2. Login or sign up
kinetics login <email>

# 3. Init a project from template
kinetics init test; cd test

# 4. View the name of the function to call locally
kinetics list

# 5. Call the function locally
kinetics invoke LibEndpoint

# 6. Edit the project name to be unique across all projects
# deployed to kinetics
vim Cargo.toml

# 7. Deploy the entire project to the cloud
kinetics deploy

# 8. Alternatively you can deploy only selected functions
kinetics deploy --functions BasicCronCron,BasicWorkerWorker
```

> Kinetics is currently in ⚠️ **active development** and may contain bugs or result in unexpected behavior. The service is free for the first **100,000 invocations** of your functions, regardless of the type of workload.
>
> If you have any issues, please contact us at support@usekinetics.com.

## Documentation

All configuration can be done through attribute macro parameters, or through modifications to `Cargo.toml` file in your project. All types of workloads support environment variables. These can be changed **without redeploying** (this feature is WIP).

#### Endpoint

The following attribute macro parameters are available:

- `url_path`: The URL path of the endpoint.
- `environment`: Environment variables.

[Example](https://github.com/ottofeller/kinetics/blob/main/examples/src/environment.rs).

#### Worker

- `concurrency`: Max number of concurrent workers.
- `fifo`: Set to true to enable FIFO processing.
- `environment`: Environment variables.
- `queue_alias`: The alias of the queue to be created for the worker. Will be added in `queues` hash map.

[Example](https://github.com/ottofeller/kinetics/blob/main/examples/src/basic/worker.rs).

#### Cron

- `schedule`: [Schedule expression](https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/aws-resource-scheduler-schedule.html#cfn-scheduler-schedule-scheduleexpression).
- `environment`: Environment variables.

[Example](https://github.com/ottofeller/kinetics/blob/main/examples/src/basic/cron.rs).

#### Secrets

Store secrets in `.env.secrets` file in the root directory of your crate. Kinetics will automatically pick it up and provision to all of your workloads in the second parameter of the function as `HashMap<String, String>`.

[Example](https://github.com/ottofeller/kinetics/blob/main/examples/src/secrets.rs).

#### Database

Database is defined in `Cargo.toml`:

```toml
[package.metadata.kinetics.kvdb.test]
# You will need this name to connect to the database
# If not defined then the resource name from above will be used as DB name
name = "test"
```

You can then interact with it like you normally interact with DynamoDB, [example](https://github.com/ottofeller/kinetics/blob/main/examples/src/database.rs).

## Commands

- `kinetics init` - Init new project from template
- `kinetics login` - Log in with email
- `kinetics invoke` - Invoke function locally
- `kinetics deploy` - Deploy your application
- `kinetics destroy` - Destroy application
- `kinetics list` – List available resources
- `kinetics logout` – Log out the current user
- `kinetics logs` - View application logs
- `kinetics stats` - View run statistics for a function

### Examples
Try in `examples/` dir. These are the most frequently used commands with examples of input params.

List out functions before deployment. Their names and URLs of REST API endpoints:
```sh
kinetics list
```
Invoke a function locally with parameters. `--payload` sets the JSON body payload:
```sh
kinetics invoke DatabaseDatabase --payload '{"account": "111", "name": "Carlos"}' --table mytable
```
Deploy entire project:
```sh
kinetics deploy
```
Deploy individual functions:
```sh
kinetics deploy --functions DatabaseDatabase,BasicWorkerWorker
```
Invoke a function remotely by automatically resolving function's name into the URL:
```sh
kinetics invoke DatabaseDatabase --remote --payload '{"account": "111", "name": "Carlos"}'
```
Output logs for a function:
```sh
kinetics logs BasicEndpointEndpoint
```
Output run statistics for a function:
```sh
kinetics stats BasicEndpointEndpoint
```


## Support & Community

- support@usekinetics.com. Help with builds, deployments, and runtime.
- [GitHub Issues](https://github.com/usekinetics/kinetics/issues). Persistent bugs, and feature requests.
