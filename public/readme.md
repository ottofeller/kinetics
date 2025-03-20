# Kinetics
Kinetics is a hosting platform for Rust applications built with the goal to be as simple, seamless, and cheap as possible.

```rust
#[endpoint(url_path = "/path", environment = {"SOME_VAR": "SomeVal"})]
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

## Features

🦀 **Only Rust code required**

No other tools needed to deploy Rust applications. Just apply attribute macro to your function, and Kinetics will handle the rest.

🚀 **Supports any workload**

With Kinetics you can deploy REST API endpoints, queue workers, and cron jobs.

🤖 **No infrastructure management**

The necessary infrastructure is provisioned automatically, e.g. a queue for the worker workload.

💿 **Comes with DB**

Seamlessly provision KV DB if your workload needs a persistent storage.

🔑 **Secrets from .env file**

Automatically provision secrets from `.env` file.

🌍 **CDN**

REST API endpoints are served through a Content Delivery Network (CDN).

🔄 **Zero-downtime updates**

Redeploy your applications without service interruption.

## Examples

Check out complete ready-to-use [examples](https://github.com/kinetics-dev/examples). There are examples for REST API endpoints, queue workers, and cron jobs.

## Try it

> [!WARNING]
> The service is currently in beta. It is in active development and may contain bugs or unexpected behavior.

> [!NOTE]
> The service will always be free for the first **100,000 invocations** of your functions, regardless of the type of workload.

1. Install
```bash
cargo install kinetics
```
2. Login
```bash
cargo kinetics login <email>
```
3. Apply one of attribute macro
```rust
// Add this to the function you want to deploy as REST API endpoint
#[endpoint()]
```
4. Deploy
```bash
# Run in the dir of your crate
cargo kinetics deploy
```
5. Test it with `curl`
```
curl <URL from cargo kinetics deploy>
```

## Documentation

All configuration can be done through attribute macro parameters, or through modifications to existing `Cargo.toml` file in your project.

> [!TIP]
> All types of workloads support environment variables. These can be changed **without redeploying**.

**Endpoint**
The following attribute macro parameters are available:

- `url_path`: The URL path of the endpoint.
- `environment`: Environment variables.

**Worker**
Attribute macro parameters:

- `concurrency`: Max number of concurrent workers.
- `fifo`: Set to true to enable FIFO processing.
- `environment`: Environment variables.

**Cron**
Attribute macro parameters:

- `schedule`: [Schedule expression](https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/aws-resource-scheduler-schedule.html#cfn-scheduler-schedule-scheduleexpression).
- `environment`: Environment variables.

**Secrets**
Store secrets in `.env.secrets` file in the root directory of your crate. Kinetics will automatically pick it up and provision to all of your workloads in the second parameter of the function as `HashMap<String, String>`.

Example:
```
# .env.secrets
API_KEY=your_api_key_here
```

```rust
#[endpoint()]
pub async fn endpoint(
    event: Request,
    secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    println!("API key: {}", secrets.get("API_KEY").unwrap());
```

**Database**
Database is defined in `Cargo.toml`:
```toml
[package.metadata.kinetics.kvdb.test]
# You will need this name to connect to the database
# If not defined then the resource name from above will be used as DB name
name = "test"
```

Connect to the database (we provision AWS DynamoDB) using the name defined in `Cargo.toml`:

```rust
#[endpoint()]
pub async fn endpoint(
    event: Request,
    secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = Client::new(&config);

    client
        .get_item()
        .table_name("test")
        .key("id", AttributeValue::S("id"))
        .send()
        .await?;
```

## Commands

- `kinetics login` - Log in with email
- `kinetics deploy` - Deploy your application
- `kinetics destroy` - Destroy application and all of its resources
- `kinetics logs` - View application logs *[Coming soon]*

## Support & Community

- support@usekinetics.com. Help with builds, deployments, and runtime.
- [GitHub Issues](https://github.com/usekinetics/kinetics/issues). Persistent bugs, and feature requests.
