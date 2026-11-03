/// Generate an HTTP entry point using this runtime version.
#[macro_export]
macro_rules! endpoint_main {
    (local, $handler:path, $url_path:expr) => {
        use http::header::{HeaderName, HeaderValue};
        use http::request::Builder;
        use std::str::FromStr;
        use $crate::tools::config::{Config as KineticsConfig, EndpointConfig};
        use $crate::tools::runtime::dependencies::{aws_config, http, serde_json, tokio, tower};
        #[tokio::main]
        async fn main() -> Result<(), tower::BoxError> {
            let user_function = $handler;
            let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
            let endpoint_config = EndpointConfig::new($url_path);
            let url_path = std::env::var("KINETICS_INVOKE_URL_PATH").unwrap_or_default();
            let url_path = if url_path.is_empty() {
                endpoint_config
                    .clone()
                    .url_pattern
                    .replace(['{', '}', '+', '*'], "")
            } else {
                url_path
            };
            let kinetics_config = KineticsConfig::new(&config, Some(endpoint_config)).await?;
            let mut secrets = std::collections::HashMap::new();

            for (k, v) in std::env::vars() {
                if k.starts_with("KINETICS_SECRET_") {
                    let key = k.replace("KINETICS_SECRET_", "");
                    secrets.insert(key, v);
                }
            }

            let payload_path = std::env::args_os().nth(1).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Payload file path is required as the first argument",
                )
            })?;

            let payload = std::fs::read_to_string(&payload_path).map_err(|err| {
                std::io::Error::new(
                    err.kind(),
                    format!(
                        "Failed to read payload file '{}': {err}",
                        std::path::Path::new(&payload_path).display(),
                    ),
                )
            })?;

            let headers_json = match std::env::var("KINETICS_INVOKE_HEADERS") {
                Ok(val) => val,
                Err(_) => "{}".into(),
            };

            let mut event_builder = Builder::new();

            if let Ok(invoke_method) = std::env::var("KINETICS_INVOKE_METHOD") {
                event_builder = event_builder.method(invoke_method.as_str());
            }

            let headers = event_builder.headers_mut().unwrap();
            let headers_value =
                serde_json::from_str::<serde_json::Value>(&headers_json).unwrap_or_default();
            let headers_obj = headers_value.as_object().unwrap();
            for (k, v) in headers_obj.iter() {
                headers.insert(
                    HeaderName::from_str(k).unwrap(),
                    HeaderValue::from_str(v.as_str().unwrap()).unwrap(),
                );
            }

            let result = match $crate::tools::http::Body::from(payload).try_into() {
                Ok(body) => {
                    let event = event_builder.uri(url_path).body(body)?;
                    user_function(event, &secrets, &kinetics_config).await
                }
                Err(error) => Err(error.into()),
            };
            match $crate::tools::http::map_result(result) {
                Ok(response) => {
                    println!("{response:?}");
                }

                Err(err) => {
                    return Err(format!("Request failed: {err:?}").into());
                }
            }

            Ok(())
        }
    };
    (remote, $handler:path, $url_path:expr) => {
        use lambda_http::{run, service_fn, Request};
        use $crate::tools::config::{Config as KineticsConfig, EndpointConfig};
        use $crate::tools::runtime::dependencies::{aws_config, http, lambda_http, tokio};
        #[tokio::main]
        async fn main() -> Result<(), lambda_http::Error> {
            let user_function = $handler;
            let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
            let runtime_config = $crate::tools::runtime::load(&config).await?;
            println!("Fetching secrets");
            let secrets = $crate::tools::secrets::load(&config, runtime_config).await?;

            let endpoint_config = EndpointConfig::new($url_path);
            let kinetics_config = KineticsConfig::new(&config, Some(endpoint_config))
                .await
                .inspect_err(|e| {
                    eprintln!("Error initializing kinetics config: {:?}", e);
                })?;

            println!("Serving requests");

            run(service_fn(|event: Request| async {
                let (head, body) = event.into_parts();
                let result = match $crate::tools::http::Body::from(body).try_into() {
                    Ok(body) => {
                        let event = http::Request::from_parts(head, body);
                        user_function(event, &secrets, &kinetics_config).await
                    }
                    Err(error) => Err(error.into()),
                };
                match $crate::tools::http::map_result(result) {
                    Ok(response) => Ok(response),
                    Err(err) => {
                        eprintln!("Error occurred while handling request: {:?}", err);
                        Err(err)
                    }
                }
            }))
            .await
        }
    };
}
