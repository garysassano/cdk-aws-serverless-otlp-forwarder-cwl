use aws_lambda_events::event::apigw::{ApiGatewayV2httpRequest, ApiGatewayV2httpResponse};
use lambda_otel_lite::{LambdaSpanProcessor, OtelTracingLayer, TelemetryConfig, init_telemetry};
use lambda_runtime::{Error, LambdaEvent, Runtime, tower::ServiceBuilder};
use opentelemetry::Context;
use opentelemetry::trace::{SpanId, Status, TraceId};
use opentelemetry_sdk::{
    Resource,
    error::OTelSdkResult,
    trace::{Span, SpanData, SpanProcessor},
};
use otlp_stdout_span_exporter::OtlpStdoutSpanExporter;
use rand::Rng;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Display};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;
// Define error type as a simple enum
#[derive(Debug)]
enum ErrorType {
    Expected,
    Unexpected,
}

impl Display for ErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// A custom span processor that implements the "wide events" pattern.
///
/// This processor accumulates attributes from all spans within a trace and attaches
/// them to the root span, creating a comprehensive view of the entire request flow.
/// This is useful for:
/// - Creating rich root spans with context from the entire trace tree
/// - Centralized logging where one span contains all relevant information
/// - Simplified querying in observability systems
///
/// ## How it works:
/// 1. **Buffer Phase**: All completed spans are held in memory (not forwarded yet)
/// 2. **Detection Phase**: Root spans are identified (spans with no parent)
/// 3. **Aggregation Phase**: When root span completes, collect all attributes from all spans
/// 4. **Enrichment Phase**: Replace root span's attributes with the aggregated set
/// 5. **Forward Phase**: Send the enriched root span + all child spans to next processor
#[derive(Debug)]
pub struct WideEventsSpanProcessor {
    /// The next processor in the chain (typically LambdaSpanProcessor -> Exporter)
    next: Box<dyn SpanProcessor>,
    /// Buffer to hold spans until the trace is complete, keyed by trace ID
    traces: Arc<Mutex<HashMap<TraceId, TraceData>>>,
}

#[derive(Debug, Default)]
struct TraceData {
    /// All spans that belong to this trace, held until root span completes
    spans: Vec<SpanData>,
    /// The span ID of the root span (the one with no parent), if we've seen it
    root_span_id: Option<SpanId>,
}

impl WideEventsSpanProcessor {
    pub fn new(next: Box<dyn SpanProcessor>) -> Self {
        Self {
            next,
            traces: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl SpanProcessor for WideEventsSpanProcessor {
    fn on_start(&self, span: &mut Span, cx: &Context) {
        // Pass through to next processor - we don't need to do anything when spans start.
        // All the work happens in on_end() when spans complete.
        self.next.on_start(span, cx);
    }

    /// **THE CORE OF THE WIDE EVENTS PATTERN**
    ///
    /// When a span ends, we don't immediately forward it to the next processor.
    /// Instead, we buffer it until the entire trace is complete, then process all spans together.
    ///
    /// ## Step-by-step process:
    /// 1. **Store the span**: Add this completed span to our trace buffer
    /// 2. **Check if root**: If this span has no parent, mark it as the root span
    /// 3. **Check for completion**: If we now have the root span, trigger aggregation
    /// 4. **Aggregate attributes**: Collect attributes from ALL spans in the trace
    /// 5. **Enrich root span**: Replace root span's attributes with the aggregated set
    /// 6. **Forward everything**: Send enriched root + all child spans to next processor
    /// 7. **Clean up**: Remove this trace from our buffer (it's done)
    fn on_end(&self, span: SpanData) {
        // Step 1: Identify this span and its trace
        let trace_id = span.span_context.trace_id();
        let is_root = span.parent_span_id == SpanId::INVALID; // Root = no parent
        let mut traces = self.traces.lock().unwrap();

        // Step 2: Add this span to the buffer for its trace
        let trace_data = traces.entry(trace_id).or_default();
        trace_data.spans.push(span);

        // Step 3: If this is the root span, remember its ID
        if is_root {
            trace_data.root_span_id = Some(trace_data.spans.last().unwrap().span_context.span_id());
        }

        // Step 4: Check if we can now process this trace (do we have the root span?)
        if let Some(root_span_id) = trace_data.root_span_id {
            // Check if the root span has actually ended (is in our buffer)
            if trace_data
                .spans
                .iter()
                .any(|s| s.span_context.span_id() == root_span_id)
            {
                // Step 5: THE ROOT SPAN IS COMPLETE! Time to aggregate and forward.

                // Remove this trace from buffer (we're done with it)
                let mut trace_data = traces.remove(&trace_id).unwrap();
                let mut root_span_idx = None;
                let mut all_attributes = HashSet::new();

                // Step 6: Collect attributes from ALL spans in this trace
                for (i, s) in trace_data.spans.iter().enumerate() {
                    if s.span_context.span_id() == root_span_id {
                        root_span_idx = Some(i); // Found the root span position
                    }
                    // Collect attributes from this span (root or child)
                    for kv in &s.attributes {
                        all_attributes.insert(kv.clone());
                    }
                }

                // Step 7: Enrich the root span with aggregated attributes
                if let Some(idx) = root_span_idx {
                    let mut root_span = trace_data.spans.remove(idx);
                    // CRITICAL: Replace root span's attributes with ALL attributes from trace
                    root_span.attributes = all_attributes.into_iter().collect();

                    // Forward the enriched root span first
                    self.next.on_end(root_span);
                }

                // Step 8: Forward all child spans (unchanged)
                for s in trace_data.spans {
                    self.next.on_end(s);
                }
            }
        }
        // If we get here, the trace isn't complete yet - just wait for more spans
    }

    fn force_flush(&self) -> OTelSdkResult {
        // Emergency flush: send any incomplete traces as-is (without aggregation)
        // This happens during shutdown or when explicitly requested
        let mut traces = self.traces.lock().unwrap();
        for (_, trace_data) in traces.drain() {
            for span in trace_data.spans {
                // Send spans without modification - we don't have complete trace
                self.next.on_end(span);
            }
        }
        self.next.force_flush()
    }

    fn shutdown(&self) -> OTelSdkResult {
        // Clean shutdown: flush any remaining traces, then shutdown next processor
        self.force_flush()?;
        self.next.shutdown()
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        // Shutdown with timeout: same as shutdown but with time limit
        self.force_flush()?;
        self.next.shutdown_with_timeout(timeout)
    }

    fn set_resource(&mut self, resource: &Resource) {
        // Pass resource configuration to next processor
        self.next.set_resource(resource);
    }
}

/// Simple nested function that creates its own span.
#[instrument(skip(event), level = "info", err)]
async fn nested_function(event: &ApiGatewayV2httpRequest) -> Result<String, ErrorType> {
    tracing::event!(
        name: "example.info",
        tracing::Level::INFO,
        "event.body" = "Nested function called",
        "event.severity_text" = "info",
        "event.severity_number" = 9
    );

    // Simulate random errors if the path is /error
    if event.raw_path.as_deref() == Some("/error") {
        let r: f64 = rand::rng().random();
        if r < 0.25 {
            return Err(ErrorType::Expected);
        } else if r < 0.5 {
            return Err(ErrorType::Unexpected);
        }
    }

    Ok("success".to_string())
}

/// Simple Hello World Lambda function using lambda-otel-lite.
///
/// This example demonstrates basic OpenTelemetry setup with lambda-otel-lite.
/// It creates spans for each invocation and logs the event payload using span events.
async fn handler(
    event: LambdaEvent<ApiGatewayV2httpRequest>,
) -> Result<ApiGatewayV2httpResponse, Error> {
    // Extract request ID from the event for correlation
    let request_id = &event.context.request_id;
    let current_span = tracing::Span::current();

    // Set request ID as span attribute
    current_span.set_attribute("request.id", request_id.to_string());

    // Log the full event payload like in Python version
    tracing::event!(
        name: "example.info",
        tracing::Level::INFO,
        "event.body" = serde_json::to_string(&event.payload).unwrap_or_default(),
        "event.severity_text" = "info",
        "event.severity_number" = 9
    );

    // Call the nested function and handle potential errors
    match nested_function(&event.payload).await {
        Ok(_) => {
            // Return a successful response
            Ok(ApiGatewayV2httpResponse {
                status_code: 200,
                body: Some(format!("Hello from request {request_id}").into()),
                ..Default::default()
            })
        }
        Err(ErrorType::Expected) => {
            // Log the error and return a 400 Bad Request
            tracing::event!(
              name:"example.error",
              tracing::Level::ERROR,
              "event.body" = "This is an expected error",
              "event.severity_text" = "error",
              "event.severity_number" = 10,
            );

            // Return a 400 Bad Request for expected errors
            Ok(ApiGatewayV2httpResponse {
                status_code: 400,
                body: Some("{{\"message\": \"This is an expected error\"}}".into()),
                ..Default::default()
            })
        }
        Err(ErrorType::Unexpected) => {
            // For other errors, propagate them up
            tracing::event!(
              name:"example.error",
              tracing::Level::ERROR,
              "event.body" = "This is an unexpected error",
              "event.severity_text" = "error",
              "event.severity_number" = 10,
            );

            // Set span status to ERROR like in Python version
            current_span.set_status(Status::Error {
                description: Cow::Borrowed("Unexpected error occurred"),
            });

            // propagate the error
            Err(Error::from("Unexpected error occurred"))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Create the OTLP stdout exporter
    let exporter = OtlpStdoutSpanExporter::default();

    // Create a LambdaSpanProcessor with the exporter
    let lambda_processor = LambdaSpanProcessor::builder().exporter(exporter).build();

    // Wrap the LambdaSpanProcessor with WideEventsSpanProcessor
    let aggregating_processor = WideEventsSpanProcessor::new(Box::new(lambda_processor));

    // Initialize telemetry with the custom processor chain
    let (_, completion_handler) = init_telemetry(
        TelemetryConfig::builder()
            .with_span_processor(aggregating_processor)
            .build(),
    )
    .await?;

    // Build service with OpenTelemetry tracing middleware
    let service = ServiceBuilder::new()
        .layer(OtelTracingLayer::new(completion_handler).with_name("tower-handler"))
        .service_fn(handler);

    // Create and run the Lambda runtime
    let runtime = Runtime::new(service);
    runtime.run().await
}
