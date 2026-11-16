/// Generate a cron entry point using this runtime version.
#[macro_export]
macro_rules! cron_main {
    (local, $handler:path) => {
        use $crate::tools::runtime::dependencies::{tokio, tower};
        use $crate::tools::runtime::{Environment, Runtime};
        #[tokio::main]
        async fn main() -> Result<(), tower::BoxError> {
            let Runtime {
                secrets,
                config: kinetics_config,
            } = Runtime::load(Environment::Local, None).await?;
            $handler(&secrets, &kinetics_config).await?;
            Ok(())
        }
    };
    (remote, $handler:path) => {
        use aws_lambda_events::eventbridge::EventBridgeEvent;
        use lambda_runtime::{Error, LambdaEvent, run, service_fn};
        use $crate::tools::runtime::dependencies::{
            aws_lambda_events, lambda_runtime, serde_json, tokio,
        };
        use $crate::tools::runtime::{Environment, Runtime};
        #[tokio::main]
        async fn main() -> Result<(), Error> {
            println!("Initializing runtime");
            let Runtime {
                secrets,
                config: kinetics_config,
            } = Runtime::load(Environment::Lambda, None).await?;
            println!("Serving requests");
            run(service_fn(
                |_event: LambdaEvent<EventBridgeEvent<serde_json::Value>>| async {
                    match $handler(&secrets, &kinetics_config).await {
                        Ok(()) => Ok(()),
                        Err(err) => {
                            eprintln!("Error occurred while handling request: {:?}", err);
                            Err(err)
                        }
                    }
                },
            ))
            .await
        }
    };
}
