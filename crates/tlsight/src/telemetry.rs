//! OpenTelemetry integration — opt-in OTLP trace export.
//!
//! When `telemetry.enabled = true` in the config, this module initializes an
//! OTLP exporter and bridges `tracing` spans into OpenTelemetry spans via
//! `tracing-opentelemetry`. When disabled, no OTel overhead is incurred.

use std::sync::OnceLock;

use opentelemetry::global;
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::{LogFormat, TelemetryConfig};

static TRACER_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

/// Initialize the tracing subscriber with an optional OpenTelemetry layer.
///
/// When `config.enabled` is true, an OTLP exporter is started and bridged
/// into the tracing subscriber. When false, only the fmt layer + env filter
/// are active — zero OTel overhead.
///
/// The caller is responsible for calling [`shutdown`] on graceful shutdown
/// to flush pending spans.
pub fn init_subscriber(config: &TelemetryConfig) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "tlsight=info,tower_http=info".into());

    let use_json = config.log_format == LogFormat::Json;

    if config.enabled {
        let otel_layer = init_otel_layer(config).expect("failed to initialize OpenTelemetry");

        if use_json {
            tracing_subscriber::registry()
                .with(otel_layer)
                .with(tracing_subscriber::fmt::layer().json())
                .with(env_filter)
                .init();
        } else {
            tracing_subscriber::registry()
                .with(otel_layer)
                .with(tracing_subscriber::fmt::layer())
                .with(env_filter)
                .init();
        }
    } else if use_json {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().json())
            .with(env_filter)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(env_filter)
            .init();
    }
}

/// Build the OpenTelemetry tracing layer.
fn init_otel_layer(
    config: &TelemetryConfig,
) -> Result<
    tracing_opentelemetry::OpenTelemetryLayer<
        tracing_subscriber::Registry,
        opentelemetry_sdk::trace::Tracer,
    >,
    Box<dyn std::error::Error + Send + Sync>,
> {
    let sampler = if (config.sample_rate - 1.0).abs() < f64::EPSILON {
        Sampler::AlwaysOn
    } else if config.sample_rate == 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.sample_rate)
    };

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(&config.otlp_endpoint)
        .build()?;

    let resource = Resource::builder()
        .with_service_name(config.service_name.clone())
        .build();

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_sampler(sampler)
        .with_resource(resource)
        .build();

    let tracer = provider.tracer("tlsight");
    global::set_tracer_provider(provider.clone());
    let _ = TRACER_PROVIDER.set(provider);

    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    Ok(layer)
}

/// Flush pending spans and shut down the global tracer provider.
pub fn shutdown() {
    if let Some(provider) = TRACER_PROVIDER.get() {
        if let Err(e) = provider.force_flush() {
            tracing::warn!(error = %e, "failed to flush OTel spans on shutdown");
        }
        if let Err(e) = provider.shutdown() {
            tracing::warn!(error = %e, "failed to shut down OTel provider");
        }
    }
}
