use chrono::{DateTime, Utc};
use lambda_extension::{Span as LambdaSpan, Status as LambdaStatus};
use opentelemetry::Value as OtelValue;
use std::collections::HashMap;

/// Represents a platform telemetry event relevant for aggregation.
/// We use an enum to clearly distinguish event types and their specific data.
#[derive(Debug, Clone)]
pub enum PlatformEventData {
    InitStart {},
    // --- Invoke Phase ---
    Start {
        version: Option<String>,
    },
    RuntimeDone {
        status: LambdaStatus,
        error_type: Option<String>,
        metrics: HashMap<String, OtelValue>,
        spans: Vec<TelemetrySpan>,
    },
    Report {
        status: LambdaStatus,
        error_type: Option<String>,
        metrics: HashMap<String, OtelValue>,
        spans: Vec<TelemetrySpan>,
    },
}

/// Structure to hold parsed platform event data passed through the channel.
#[derive(Debug, Clone)]
pub struct ParsedPlatformEvent {
    pub timestamp: DateTime<Utc>,
    pub request_id: String,
    pub data: PlatformEventData,
}

/// Represents a span reported within a Lambda Telemetry event record.
#[derive(Clone, Debug, PartialEq)]
pub struct TelemetrySpan {
    pub duration_ms: f64,
    pub name: String,
    pub start: DateTime<Utc>,
}

// Helper to convert lambda_extension::Span to our TelemetrySpan
impl From<LambdaSpan> for TelemetrySpan {
    fn from(span: LambdaSpan) -> Self {
        Self {
            duration_ms: span.duration_ms,
            name: span.name,
            start: span.start,
        }
    }
}
