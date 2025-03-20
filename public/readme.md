# Kinetics
Kinetics is a hosting platform for Rust applications built with the goal to be as simple, seamless, and cheap as possible.

Deploying to Kinetics is as simple as assigning an attribute macro:

```rust
use kinetics_macro::endpoint;
use lambda_http::{Body, Error, Request, RequestExt, Response};
use std::collections::HashMap;

// Deploy a REST API endpoint
#[endpoint(url_path = "/path", environment = {"SOME_VAR": "SomeVal"})]
pub async fn endpoint(
    _event: Request,
    _secrets: &HashMap<String, String>,
) -> Result<Response<Body>, Error> {
    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body("Hello!".into())
        .map_err(Box::new)?;

    Ok(resp)
}
```

## Features

🦀 **Only Rust code required**

No other tools needed to deploy Rust applications. Just assign an attribute macro to your function, and Kinetics will handle the rest.

🚀 **Supports any workload**

With Kinetics you can deploy REST API endpoints, queue workers, and cron jobs.

🤖 **No infrastructure management**

The necessary infrastructure is provisioned automatically, e.g. a queue for the worker workload.

🌍 **CDN**

REST API endpoints are served through a Content Delivery Network (CDN) for fast and reliable delivery.

🔄 **Zero-downtime updates**

Update your applications without service interruption.

## Examples

Check out complete ready-to-use [examples](https://github.com/kinetics-dev/examples). There are examples for REST API endpoints, queue workers, and cron jobs.

## Try it

> [!WARNING]
> The service is currently in beta. It is in active development and may contain bugs or unexpected behavior.

> [!NOTE]
> The service will always be free for the first **100,000 invocations** of your functions, regardless of the type of workload.

1. Install CLI
2. Login
3. Assign attribute macro
4. Deploy
5. Test it with `curl`.

## Documentation

There is no extensive documentation at the moment. [Examples](https://github.com/kinetics-dev/examples) should give you a good understanding of how to use Kinetics.

## Configuration

All configuration can be done through attribute macro parameters, or by editing existing `Cargo.toml` file in your project.

**Endpoint**

**Worker**

**Cron**

## Commands

- `kinetics login` - Log in with email
- `kinetics deploy` - Deploy your application
- `kinetics destroy` - Destroy application and all of its resources
- `kinetics logs` - View application logs *[Coming soon]*

## Support & Community

- support@usekinetics.com. Help with builds, deployments, and runtime.
- [GitHub Issues](https://github.com/usekinetics/kinetics/issues). Persistent bugs, and feature requests.
