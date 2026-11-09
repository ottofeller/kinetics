/// Render a scheduled handler entry point for a local call or EventBridge events.
pub(super) fn render(import_statement: &str, rust_function_name: &str, is_local: bool) -> String {
    // For local run we create a dedicated crate, without cargo lambda wrappers
    if is_local {
        format!(
            "{import_statement}
            use kinetics_lib::tools::config::Config as KineticsConfig;
            #[tokio::main]\n\
            async fn main() -> Result<(), tower::BoxError> {{\n\
                let user_function = {rust_function_name};
                let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                let kinetics_config = KineticsConfig::new(&config, None).await?;
                let mut secrets = std::collections::HashMap::new();

                for (k, v) in std::env::vars() {{
                    if k.starts_with(\"KINETICS_SECRET_\") {{
                        let key = k.replace(\"KINETICS_SECRET_\", \"\");
                        secrets.insert(key, v);
                    }}
                }}

                user_function(&secrets, &kinetics_config).await?;
                Ok(())
            }}\n\n"
        )
    } else {
        format!(
            "{import_statement}
            use kinetics_lib::tools::config::Config as KineticsConfig;
            use lambda_runtime::{{LambdaEvent, Error, run, service_fn}};\n\
            use aws_lambda_events::eventbridge::EventBridgeEvent;\n\
            #[tokio::main]\n\
            async fn main() -> Result<(), Error> {{\n\
                let user_function = {rust_function_name};
                let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                let runtime_config = kinetics_lib::tools::runtime::load(&config).await?;
                println!(\"Provisioning secrets\");
                let secrets = kinetics_lib::tools::secrets::load(&config, runtime_config).await?;

                let kinetics_config = KineticsConfig::new(&config, None).await?;
                println!(\"Serving requests\");

                run(service_fn(|_event: LambdaEvent<EventBridgeEvent<serde_json::Value>>| async {{
                    match user_function(&secrets, &kinetics_config).await {{
                        Ok(()) => Ok(()),
                        Err(err) => {{
                            eprintln!(\"Error occurred while handling request: {{:?}}\", err);
                            Err(err)
                        }}
                    }}
                }}))
                .await
            }}\n\n"
        )
    }
}
