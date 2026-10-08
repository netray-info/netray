//! Shared telemetry initialisation — tracing-subscriber + optional OTel OTLP export.
//!
//! When `config.enabled = true`, an OTLP exporter is started and bridged into
//! the tracing subscriber. When disabled, only the fmt layer + env filter are
//! active — zero OTel overhead.

use std::sync::OnceLock;
use std::time::Duration;

use opentelemetry::global;
use opentelemetry::trace::TracerProvider;
use opentelemetry_http::{Bytes, HttpClient, HttpError, Request, Response};
use opentelemetry_otlp::{WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use serde::Deserialize;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

static TRACER_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

/// OTLP transport over reqwest 0.13's blocking client.
///
/// `opentelemetry-http` 0.31 implements `HttpClient` only for reqwest 0.12; this
/// mirrors its blocking impl so the workspace resolves a single reqwest.
#[derive(Debug)]
struct BlockingReqwestClient(reqwest::blocking::Client);

#[async_trait::async_trait]
impl HttpClient for BlockingReqwestClient {
    async fn send_bytes(&self, request: Request<Bytes>) -> Result<Response<Bytes>, HttpError> {
        let request = request.try_into()?;
        let mut response = self.0.execute(request)?.error_for_status()?;
        let headers = std::mem::take(response.headers_mut());
        let mut http_response = Response::builder()
            .status(response.status())
            .body(response.bytes()?)?;
        *http_response.headers_mut() = headers;
        Ok(http_response)
    }
}

/// Build the blocking client on its own thread: `reqwest::blocking` panics when
/// created or dropped inside an async runtime.
fn build_http_client() -> Result<BlockingReqwestClient, Box<dyn std::error::Error + Send + Sync>> {
    let timeout = otlp_export_timeout(|k| std::env::var(k).ok());
    let client = std::thread::spawn(move || {
        reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
    })
    .join()
    .map_err(|_| "OTLP HTTP client thread panicked")??;
    Ok(BlockingReqwestClient(client))
}

/// Export timeout as opentelemetry-otlp 0.31.1 resolves it: the traces
/// variable, then the generic one (milliseconds; unparsable counts as unset),
/// then 10 s.
fn otlp_export_timeout(var: impl Fn(&str) -> Option<String>) -> Duration {
    [
        "OTEL_EXPORTER_OTLP_TRACES_TIMEOUT",
        "OTEL_EXPORTER_OTLP_TIMEOUT",
    ]
    .iter()
    .find_map(|name| var(name).and_then(|v| v.parse::<u64>().ok()))
    .map(Duration::from_millis)
    .unwrap_or(Duration::from_secs(10))
}

/// Log output format.
///
/// Configurable via `telemetry.log_format` in the service TOML config.
///
/// - `text` (default): human-readable, colour-coded output for local development.
/// - `json`: structured JSON lines for log aggregators (Loki, CloudWatch, Datadog).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Text,
    Json,
}

/// Telemetry configuration shared across services.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_otlp_endpoint")]
    pub otlp_endpoint: String,
    #[serde(default)]
    pub service_name: String,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: f64,
    #[serde(default)]
    pub log_format: LogFormat,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            otlp_endpoint: default_otlp_endpoint(),
            service_name: String::new(),
            sample_rate: default_sample_rate(),
            log_format: LogFormat::default(),
        }
    }
}

fn default_otlp_endpoint() -> String {
    "http://localhost:4318".to_owned()
}

fn default_sample_rate() -> f64 {
    1.0
}

/// Initialise the tracing subscriber with an optional OpenTelemetry layer.
///
/// `default_filter` is used as the fallback when `RUST_LOG` is not set
/// (e.g. `"prism=info,tower_http=info"`).
///
/// The caller is responsible for calling [`shutdown`] on graceful shutdown
/// to flush pending spans.
pub fn init_subscriber(config: &TelemetryConfig, default_filter: &str) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| default_filter.into());

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

    let log_format = match config.log_format {
        LogFormat::Text => "text",
        LogFormat::Json => "json",
    };

    if config.enabled && !config.service_name.is_empty() {
        tracing::info!(
            log_format,
            otel_enabled = true,
            otel_endpoint = %config.otlp_endpoint,
            service_name = %config.service_name,
            "telemetry initialized"
        );
    } else if config.enabled {
        tracing::info!(
            log_format,
            otel_enabled = true,
            otel_endpoint = %config.otlp_endpoint,
            "telemetry initialized"
        );
    } else if !config.service_name.is_empty() {
        tracing::info!(
            log_format,
            otel_enabled = false,
            service_name = %config.service_name,
            "telemetry initialized"
        );
    } else {
        tracing::info!(log_format, otel_enabled = false, "telemetry initialized");
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
        .with_http_client(build_http_client()?)
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

    let tracer = provider.tracer(config.service_name.clone());
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

#[cfg(test)]
mod tests {
    use super::otlp_export_timeout;
    use std::collections::HashMap;
    use std::time::Duration;

    fn timeout_with(vars: &[(&str, &str)]) -> Duration {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        otlp_export_timeout(|name| map.get(name).cloned())
    }

    #[test]
    fn otlp_export_timeout_defaults_to_10s_without_vars() {
        assert_eq!(timeout_with(&[]), Duration::from_secs(10));
    }

    #[test]
    fn otlp_export_timeout_uses_generic_var_in_millis() {
        assert_eq!(
            timeout_with(&[("OTEL_EXPORTER_OTLP_TIMEOUT", "2000")]),
            Duration::from_secs(2)
        );
    }

    #[test]
    fn otlp_export_timeout_traces_var_wins_over_generic() {
        assert_eq!(
            timeout_with(&[
                ("OTEL_EXPORTER_OTLP_TIMEOUT", "2000"),
                ("OTEL_EXPORTER_OTLP_TRACES_TIMEOUT", "500"),
            ]),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn otlp_export_timeout_unparsable_traces_falls_back_to_generic() {
        assert_eq!(
            timeout_with(&[
                ("OTEL_EXPORTER_OTLP_TIMEOUT", "2000"),
                ("OTEL_EXPORTER_OTLP_TRACES_TIMEOUT", "soon"),
            ]),
            Duration::from_secs(2)
        );
    }

    #[test]
    fn otlp_export_timeout_unparsable_everywhere_falls_back_to_default() {
        assert_eq!(
            timeout_with(&[
                ("OTEL_EXPORTER_OTLP_TIMEOUT", "-1"),
                ("OTEL_EXPORTER_OTLP_TRACES_TIMEOUT", "2s"),
            ]),
            Duration::from_secs(10)
        );
    }
}
