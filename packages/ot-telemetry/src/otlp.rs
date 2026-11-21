//! OTLP/HTTP export (feature `otlp`). Every OpenTelemetry SDK and exporter
//! API call lives in this module: those crates are pre-1.0 and change between
//! minors, so upgrades should only touch this file.

use crate::config::{Resolved, ResolvedOtlp};
use opentelemetry::KeyValue;
use opentelemetry_http::{Bytes, HttpClient, HttpError, Request, Response};
use opentelemetry_otlp::{Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use std::collections::HashMap;
use tracing::Subscriber;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

/// Instrumentation scope name for spans created through `tracing`.
const TRACER_SCOPE: &str = "one-terminal";

/// The SDK providers of one process. Cloning shares them.
#[derive(Clone)]
pub(crate) struct Providers {
    pub tracer: SdkTracerProvider,
    pub meter: Option<SdkMeterProvider>,
    pub logger: Option<SdkLoggerProvider>,
}

impl Providers {
    /// A tracer provider with no exporter: spans still get trace ids (for log
    /// lines and `TRACEPARENT`), but nothing is recorded or sent.
    pub fn local_only(resolved: &Resolved, version: &str) -> Self {
        Self {
            tracer: SdkTracerProvider::builder()
                .with_sampler(Sampler::AlwaysOff)
                .with_resource(resource(resolved, version))
                .build(),
            meter: None,
            logger: None,
        }
    }

    /// Providers exporting to the endpoints in `otlp`. A signal whose
    /// exporter can't be built is skipped with an error in the returned list;
    /// the others still export.
    pub fn exporting(
        resolved: &Resolved,
        otlp: &ResolvedOtlp,
        version: &str,
    ) -> (Self, Vec<String>) {
        let resource = resource(resolved, version);
        let mut errors = Vec::new();
        let client = match BlockingClient::new(otlp) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("OTLP HTTP client: {e}; export disabled"));
                return (Self::local_only(resolved, version), errors);
            }
        };
        let headers: HashMap<String, String> = otlp.headers.0.clone().into_iter().collect();

        let mut tracer = SdkTracerProvider::builder().with_resource(resource.clone());
        let mut tracing_exports = false;
        if let Some(url) = &otlp.traces_endpoint {
            match opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .with_protocol(Protocol::HttpBinary)
                .with_endpoint(url)
                .with_timeout(otlp.timeout)
                .with_headers(headers.clone())
                .with_http_client(client.clone())
                .build()
            {
                Ok(exporter) => {
                    tracer = tracer.with_batch_exporter(exporter);
                    tracing_exports = true;
                }
                Err(e) => errors.push(format!("OTLP trace exporter: {e}")),
            }
        }
        let sampler = if tracing_exports {
            Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(otlp.sample_ratio)))
        } else {
            Sampler::AlwaysOff
        };
        let tracer = tracer.with_sampler(sampler).build();

        let meter = otlp.metrics_endpoint.as_ref().and_then(|url| {
            match opentelemetry_otlp::MetricExporter::builder()
                .with_http()
                .with_protocol(Protocol::HttpBinary)
                .with_endpoint(url)
                .with_timeout(otlp.timeout)
                .with_headers(headers.clone())
                .with_http_client(client.clone())
                .build()
            {
                Ok(exporter) => Some(
                    SdkMeterProvider::builder()
                        .with_resource(resource.clone())
                        .with_reader(
                            PeriodicReader::builder(exporter)
                                .with_interval(otlp.metrics_interval)
                                .build(),
                        )
                        .build(),
                ),
                Err(e) => {
                    errors.push(format!("OTLP metric exporter: {e}"));
                    None
                }
            }
        });

        let logger = otlp.logs_endpoint.as_ref().and_then(|url| {
            match opentelemetry_otlp::LogExporter::builder()
                .with_http()
                .with_protocol(Protocol::HttpBinary)
                .with_endpoint(url)
                .with_timeout(otlp.timeout)
                .with_headers(headers.clone())
                .with_http_client(client.clone())
                .build()
            {
                Ok(exporter) => Some(
                    SdkLoggerProvider::builder()
                        .with_resource(resource.clone())
                        .with_batch_exporter(exporter)
                        .build(),
                ),
                Err(e) => {
                    errors.push(format!("OTLP log exporter: {e}"));
                    None
                }
            }
        });

        (
            Self {
                tracer,
                meter,
                logger,
            },
            errors,
        )
    }

    /// True when at least one signal leaves the process.
    pub fn exports(&self, otlp: Option<&ResolvedOtlp>) -> bool {
        self.meter.is_some()
            || self.logger.is_some()
            || otlp.is_some_and(|o| o.traces_endpoint.is_some())
    }

    /// `tracing` layers: spans → OpenTelemetry, and (when logs export) events
    /// → OTLP log records. The log bridge skips the exporter's own HTTP stack,
    /// which would otherwise export its own logs in a loop.
    pub fn layers<S>(&self) -> Vec<Box<dyn Layer<S> + Send + Sync>>
    where
        S: Subscriber + for<'a> LookupSpan<'a> + Send + Sync,
    {
        use opentelemetry::trace::TracerProvider as _;
        let mut layers: Vec<Box<dyn Layer<S> + Send + Sync>> = vec![Box::new(
            tracing_opentelemetry::layer().with_tracer(self.tracer.tracer(TRACER_SCOPE)),
        )];
        if let Some(logger) = &self.logger {
            let bridge =
                opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(logger)
                    .with_filter(filter_fn(|meta| !is_exporter_target(meta.target())));
            layers.push(Box::new(bridge));
        }
        layers
    }

    /// One flush job per provider, logs first: they're usually the most
    /// useful tail. Run them in parallel so a slow signal can't starve the
    /// others within the shared cap.
    pub fn flush_jobs(&self) -> Vec<Job> {
        self.jobs(|p| match p {
            Provider::Tracer(t) => {
                let _ = t.force_flush();
            }
            Provider::Meter(m) => {
                let _ = m.force_flush();
            }
            Provider::Logger(l) => {
                let _ = l.force_flush();
            }
        })
    }

    /// One shutdown job per provider (see [`Self::flush_jobs`]).
    pub fn shutdown_jobs(&self) -> Vec<Job> {
        self.jobs(|p| match p {
            Provider::Tracer(t) => {
                let _ = t.shutdown();
            }
            Provider::Meter(m) => {
                let _ = m.shutdown();
            }
            Provider::Logger(l) => {
                let _ = l.shutdown();
            }
        })
    }

    fn jobs(&self, run: fn(Provider)) -> Vec<Job> {
        let mut providers = Vec::new();
        if let Some(l) = &self.logger {
            providers.push(Provider::Logger(l.clone()));
        }
        providers.push(Provider::Tracer(self.tracer.clone()));
        if let Some(m) = &self.meter {
            providers.push(Provider::Meter(m.clone()));
        }
        providers
            .into_iter()
            .map(|p| Box::new(move || run(p)) as Job)
            .collect()
    }
}

/// Work run on a helper thread under [`crate::FLUSH_CAP`].
pub(crate) type Job = Box<dyn FnOnce() + Send>;

enum Provider {
    Tracer(SdkTracerProvider),
    Meter(SdkMeterProvider),
    Logger(SdkLoggerProvider),
}

/// Targets of the exporter's HTTP stack.
fn is_exporter_target(target: &str) -> bool {
    ["opentelemetry", "reqwest", "hyper", "h2", "rustls"]
        .iter()
        .any(|prefix| target.starts_with(prefix))
}

/// `service.*`, `os.type` and config attributes. `OTEL_RESOURCE_ATTRIBUTES`
/// (read by the SDK's resource detectors) wins over config attributes.
fn resource(resolved: &Resolved, version: &str) -> Resource {
    let env_keys: Vec<String> = std::env::var("OTEL_RESOURCE_ATTRIBUTES")
        .unwrap_or_default()
        .split(',')
        .filter_map(|kv| kv.split_once('=').map(|(k, _)| k.trim().to_string()))
        .collect();
    let mut attrs = vec![
        KeyValue::new("service.namespace", resolved.service_namespace.clone()),
        KeyValue::new("service.version", version.to_string()),
        KeyValue::new("service.instance.id", crate::instance_id().to_string()),
        KeyValue::new("os.type", os_type()),
    ];
    attrs.extend(
        resolved
            .resource_attributes
            .iter()
            .filter(|(k, _)| !env_keys.iter().any(|e| e == *k))
            .map(|(k, v)| KeyValue::new(k.clone(), v.clone())),
    );
    Resource::builder()
        .with_service_name(resolved.service_name.clone())
        .with_attributes(attrs)
        .build()
}

/// OpenTelemetry semantic-convention value for `os.type`.
fn os_type() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
}

/// Largest collector response body read (the exporter's own clients cap at
/// 4 MiB; OTLP responses are tiny).
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

/// OTLP HTTP client over the workspace's reqwest 0.12 blocking client. The
/// SDK's batch processors and periodic reader each run on their own thread and
/// drive exports with `block_on`, so a blocking send here never stalls an
/// async runtime.
#[derive(Clone, Debug)]
struct BlockingClient(reqwest::blocking::Client);

impl BlockingClient {
    fn new(otlp: &ResolvedOtlp) -> Result<Self, reqwest::Error> {
        reqwest::blocking::Client::builder()
            .timeout(otlp.timeout)
            // OTLP never needs redirects, and following one would resend the
            // telemetry and any API-key headers to another host.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map(Self)
    }
}

#[async_trait::async_trait]
impl HttpClient for BlockingClient {
    async fn send_bytes(&self, request: Request<Bytes>) -> Result<Response<Bytes>, HttpError> {
        use std::io::Read;
        let (parts, body) = request.into_parts();
        let response = self
            .0
            .request(parts.method, parts.uri.to_string())
            .headers(parts.headers)
            .body(reqwest::blocking::Body::from(body))
            .send()?;
        let status = response.status();
        let headers = response.headers().clone();
        // The exporter only needs the status (and, rarely, a small partial-
        // success body); never buffer an arbitrarily large response.
        let mut body = Vec::new();
        response.take(MAX_RESPONSE_BYTES).read_to_end(&mut body)?;
        let mut out = Response::builder().status(status).body(Bytes::from(body))?;
        *out.headers_mut() = headers;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exporter_targets_are_filtered_from_the_log_bridge() {
        for t in [
            "opentelemetry_sdk",
            "reqwest::connect",
            "hyper_util::client",
            "h2::codec",
        ] {
            assert!(is_exporter_target(t), "{t}");
        }
        for t in ["desktop_agent::fdc3_bus", "wm", "one_terminal"] {
            assert!(!is_exporter_target(t), "{t}");
        }
    }
}
