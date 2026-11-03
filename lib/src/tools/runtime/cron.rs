/// Generate a cron entry point using this runtime version.
#[macro_export]
macro_rules! cron_main {
    (local, $handler:path) => {
        use $crate::tools::config::Config as KineticsConfig;
        use $crate::tools::runtime::dependencies::{aws_config, tokio, tower};
        #[tokio::main]
        async fn main() -> Result<(), tower::BoxError> {
            let user_function = $handler;
            let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
            let kinetics_config = KineticsConfig::new(&config, None).await?;
            let mut secrets = std::collections::HashMap::new();

            for (k, v) in std::env::vars() {
                if k.starts_with("KINETICS_SECRET_") {
                    let key = k.replace("KINETICS_SECRET_", "");
                    secrets.insert(key, v);
                }
            }

            user_function(&secrets, &kinetics_config).await?;
            Ok(())
        }
    };
    (remote, $handler:path) => {
        use aws_lambda_events::eventbridge::EventBridgeEvent;
        use lambda_runtime::{run, service_fn, Error, LambdaEvent};
        use $crate::tools::config::Config as KineticsConfig;
        use $crate::tools::runtime::dependencies::{
            aws_config, aws_lambda_events, lambda_runtime, serde_json, tokio,
        };
        #[tokio::main]
        async fn main() -> Result<(), Error> {
            let user_function = $handler;
            let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
            let runtime_config = $crate::tools::runtime::load(&config).await?;
            println!("Provisioning secrets");
            let secrets = $crate::tools::secrets::load(&config, runtime_config).await?;

            let kinetics_config = KineticsConfig::new(&config, None).await?;
            println!("Serving requests");

            run(service_fn(
                |_event: LambdaEvent<EventBridgeEvent<serde_json::Value>>| async {
                    match user_function(&secrets, &kinetics_config).await {
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
