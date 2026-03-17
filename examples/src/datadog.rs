use http::{Request, Response};
use kinetics::tools::config::Config as KineticsConfig;
use kinetics::{macros::endpoint, tools::http::Body};
use opentelemetry::global;
use opentelemetry::KeyValue;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, MetricExporter};
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::{SdkMeterProvider, Temporality};
use opentelemetry_sdk::Resource;
use serde_json::json;
use std::collections::HashMap;
use std::sync::OnceLock;
use tower::BoxError;
use tracing::{error, info};
use tracing_subscriber::prelude::*;
static RESOURCE: OnceLock<Resource> = OnceLock::new();
static LOGGER: OnceLock<()> = OnceLock::new();
static METER: OnceLock<()> = OnceLock::new();

fn resource() -> Resource {
    RESOURCE
        .get_or_init(|| {
            Resource::builder()
                .with_service_name("kinetics-datadog-example")
                .build()
        })
        .clone()
}

fn init_metrics() {
    let exporter = MetricExporter::builder()
        .with_http()
        // DataDog metrics intake requirement
        .with_temporality(Temporality::Delta)
        .build()
        .expect("Failed to create metric exporter");

    global::set_meter_provider(
        SdkMeterProvider::builder()
            .with_periodic_exporter(exporter)
            .with_resource(resource())
            .build(),
    );
}

fn init_logs() {
    let exporter = LogExporter::builder()
        .with_http()
        .build()
        .expect("Failed to create log exporter");

    let provider = SdkLoggerProvider::builder()
        .with_resource(resource())
        .with_batch_exporter(exporter)
        .build();

    let otel_layer = OpenTelemetryTracingBridge::new(&provider);
    tracing_subscriber::registry().with(otel_layer).init();
}

/// DataDog logs and metrics example
///
/// Data is sent to DataDog's OTLP intake endpoints. Traces are not presented in the example,
/// as DataDog currently does not publicly provide OTLP intake endpoint for them.
///
/// Test remotely with the following command:
/// kinetics invoke DatadogDatadog --remote
#[endpoint(url_path = "/datadog")]
pub async fn datadog(
    _event: Request<Body>,
    _secrets: &HashMap<String, String>,
    _config: &KineticsConfig,
) -> Result<Response<String>, BoxError> {
    LOGGER.get_or_init(init_logs);
    METER.get_or_init(init_metrics);
    let meter = global::meter("meter");
    let counter = meter.u64_counter("test_counter").build();

    for _ in 0..10 {
        counter.add(1, &[KeyValue::new("test_key", "test_value")]);
    }

    info!("This is an info message");
    error!("Error!");

    let resp = Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(json!({"success": true}).to_string())?;

    Ok(resp)
}
