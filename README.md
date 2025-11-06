# Kinetics

Kinetics is a hosting platform for Rust applications that allows deploying all types of workloads by writing **only Rust code**.

```rust
#[endpoint(
    url_path = "/path",
    environment = {"SOME_VAR": "SomeVal"},
)]
pub async fn endpoint(
    _event: Request<()>,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, BoxError> {
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

Run your functions locally, with no connection to the internet. Requests to DB and queues are served locally as well.

💿 **Comes with DB**

For every project we provision a DB, with connection string being added to function config automatically.

📥 **Comes with queues**

When you launch a worker function, we automatically provision its queue. Other functions access the queue by simply importing the worker function.

🔑 **Secrets**

Automatically provision secrets from `.env.secrets` file and make it available in your functions.

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
```

> Kinetics is currently in ⚠️ **active development** and may contain bugs or result in unexpected behavior. The service is free for the first **100,000 invocations** of your functions, regardless of the type of workload.
>
> If you have any issues, please contact us at support@kineticscloud.com.

## Documentation

#### Endpoint

The following attribute macro parameters are available:

- `url_path`: The URL path of the endpoint.
- `environment`: Environment variables.

[Example](https://github.com/ottofeller/kinetics/blob/main/examples/src/environment.rs).

#### Worker

- `concurrency`: Max number of concurrent workers.
- `fifo`: Set to true to enable FIFO processing.
- `environment`: Environment variables.

[Example](https://github.com/ottofeller/kinetics/blob/main/examples/src/basic/worker.rs).

#### Cron

- `schedule`: [Schedule expression](https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/aws-resource-scheduler-schedule.html#cfn-scheduler-schedule-scheduleexpression).
- `environment`: Environment variables.

[Example](https://github.com/ottofeller/kinetics/blob/main/examples/src/basic/cron.rs).

#### Env vars

A macro for any type of workload accepts JSON array with environment variables.

[Example](https://github.com/ottofeller/kinetics/blob/8cab4e6719b7dea944459ca59a82935d5e30e074/examples/src/environment.rs).

#### Secrets

Store secrets in `.env.secrets` file in the root directory of your crate. Kinetics will automatically pick it up and provision to all of your workloads in the second parameter of the function as `HashMap<String, String>`.

[Example](https://github.com/ottofeller/kinetics/blob/main/examples/src/secrets.rs).

#### Database

We automatically provision one SQL DB for each project. Also `kinetics invoke --with-db [Function name]` will automatically provision DB locally and replace the connection string with local endpoint.

You can then interact with it like you normally interact with DynamoDB, [example](https://github.com/ottofeller/kinetics/blob/main/examples/src/database.rs).

## Commands

- `kinetics init` - Init new project from template
- `kinetics login` - Log in with email
- `kinetics logout` – Log out the current user
- `kinetics invoke` - Invoke function locally
- `kinetics deploy` - Deploy your application
- `kinetics proj destroy` - Destroy application
- `kinetics proj rollback` - Rollback project to previous version
- `kinetics proj versions` - Show all versions of the project
- `kinetics proj list` - Show all user's projects
- `kinetics func list` - List available resources
- `kinetics func stats` - View run statistics for a function
- `kinetics func logs` - View application logs

### Examples

Try in `examples/` dir. These are the most frequently used commands with examples of input params.

Print out all available functions before deployment. Their names and URLs of REST API endpoints:

```sh
kinetics func list
```

Invoke a function locally with parameters. `--payload` sets the JSON body payload:

```sh
kinetics invoke BasicWorkerWorker --payload '{"name": "John"}'
```

Invoke a function which needs a DB. DB gets provisioned locally and is fully operational, not just a mock for requests.

```sh
kinetics invoke DatabaseDatabase --with-db
```

Deploy entire project:

```sh
kinetics deploy
```

Deploy individual functions:

```sh
kinetics deploy DatabaseDatabase,BasicWorkerWorker
```

Invoke a function remotely by automatically resolving function's name into the URL:

```sh
kinetics invoke DatabaseDatabase --remote
```

Output logs for a function:

```sh
kinetics func logs BasicEndpointEndpoint
```

Output run statistics for a function:

```sh
kinetics func stats BasicEndpointEndpoint
```

## Support & Community

- support@usekinetics.com. Help with builds, deployments, and runtime.
- [GitHub Issues](https://github.com/usekinetics/kinetics/issues). Persistent bugs, and feature requests.
