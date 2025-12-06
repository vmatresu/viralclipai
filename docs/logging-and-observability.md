# Logging & Observability (Rust)

This document explains how logging, metrics, and tracing work in the current Rust-based Viral Clip AI backend, and how they connect to Prometheus + Grafana.

## Logging

### Rust Backend

The Rust crates use the `tracing` ecosystem for structured logs:

- `tracing` – instrumentation macros (`info!`, `warn!`, `error!`, `span!`)
- `tracing-subscriber` – log formatting + env-based filters
- `tracing-opentelemetry` – bridge to OpenTelemetry when enabled

Typical behavior:

- Log level controlled by an env var such as `RUST_LOG` or a config setting
- JSON or human-readable output depending on subscriber configuration
- Contextual fields (job IDs, video IDs, user IDs) included in logs for easy correlation

Examples (conceptual):

```rust
tracing::info!(job_id = %job.job_id, "processing video job");
tracing::warn!(video_id = %video_id, "highlights not found, status = {:?}", video_meta.status);
```

Logs should be shipped from containers to your central logging solution (CloudWatch, Loki, Datadog, etc.).

### Frontend

The Next.js app uses a small abstraction (e.g. `web/lib/logger`) rather than raw `console.log`:

- `info`, `warn`, `error` helpers
- Can be wired to an error-tracking service (Sentry, LogRocket, etc.)

Key integration points:

- Processing UI (WebSocket events, failures)
- Auth flows (Firebase issues)
- Clip management (downloads, TikTok publish attempts)

## Metrics & Prometheus

The backend uses the `metrics` crate and `metrics-exporter-prometheus` to expose Prometheus-compatible metrics.

Typical metrics:

- Job completion rate
- Processing duration per video and per clip
- Error counts by type
- Active worker count

A Prometheus scrape endpoint is usually exposed on an internal port and configured via `monitoring/prometheus.yml`.

### Prometheus & Grafana

Under `monitoring/` you will find:

- `prometheus/prometheus.yml` – scrape configuration
- `grafana/dashboards/` – example dashboards
- `grafana/provisioning/` – provisioning for datasources/dashboards

Recommended metrics to visualize:

- Request rate and error rate for the API
- Worker job throughput
- Average processing time per job
- FFmpeg failures and retry counts

## Tracing & Distributed Context

With `opentelemetry` + `tracing-opentelemetry`, the backend can export traces to OTLP-compatible backends (Tempo, Jaeger, etc.).

Key pieces:

- `OPENTELEMETRY` setup in the API/worker main function
- `OTEL_EXPORTER_OTLP_ENDPOINT` env var (and related) to point to your collector
- Spans around critical sections (Gemini calls, FFmpeg execution, Firestore operations)

Example (conceptual):

```rust
let span = tracing::info_span!("process_video", video_id = %job.video_id);
let _enter = span.enter();
// perform work here
```

## Error Handling Strategy

The Rust backend follows these principles:

- **Fail fast for invalid input** – early `400`/`422` responses
- **Sanitize errors in production** – generic messages for clients, detailed logs for operators
- **Differentiate transient vs permanent errors** – so workers can decide between retry/fail
- **Always clean up temp resources** – work directories deleted even on failure

See `docs/video-processing-pipeline.md` for error-handling examples in the worker.

## Operational Recommendations

- **Log format**: prefer structured logs (JSON) in production for easier parsing
- **Correlation IDs**: include job IDs / request IDs in all logs and traces
- **Dashboards**: maintain Grafana dashboards for API latency, error rates, worker throughput, and queue depth
- **Alerts**: set alerts on error rates, job latency, queue backlog, and storage/Firestore failures

For deployment-time considerations, see `docs/deployment.md`.
