import {
  createTracedHandler,
  defaultExtractor,
  initTelemetry,
  TriggerType,
} from "@dev7a/lambda-otel-lite";
import { SpanKind, SpanStatusCode, trace } from "@opentelemetry/api";
import { registerInstrumentations } from "@opentelemetry/instrumentation";
import { AwsInstrumentation } from "@opentelemetry/instrumentation-aws-sdk";
import { HttpInstrumentation } from "@opentelemetry/instrumentation-http";
import { UndiciInstrumentation } from "@opentelemetry/instrumentation-undici";
import {
  APIGatewayProxyStructuredResultV2,
  Context as LambdaContext,
  ScheduledEvent,
} from "aws-lambda";
import { z } from "zod";
import { validateEnv } from "../../utils/validate-env";

//==============================================================================
// LAMBDA INITIALIZATION (COLD START)
//==============================================================================

// Initialize telemetry with default configuration
// The service name will be automatically set from OTEL_SERVICE_NAME
// or AWS_LAMBDA_FUNCTION_NAME environment variables
const { tracer, completionHandler } = initTelemetry();

// Register instrumentations
registerInstrumentations({
  tracerProvider: trace.getTracerProvider(),
  instrumentations: [
    new AwsInstrumentation(),
    new HttpInstrumentation(),
    new UndiciInstrumentation(),
  ],
});

// Define API endpoints
const QUOTES_URL = "https://dummyjson.com/quotes/random";
const { TARGET_URL } = validateEnv(["TARGET_URL"]);

// Define the schema for quote validation
const QuoteSchema = z.object({
  id: z.number(),
  quote: z.string(),
  author: z.string(),
});
type Quote = z.infer<typeof QuoteSchema>;

//==============================================================================
// LAMBDA HANDLER
//==============================================================================

async function lambdaHandler(
  _event: ScheduledEvent,
  _context: LambdaContext,
): Promise<APIGatewayProxyStructuredResultV2> {
  const currentSpan = trace.getActiveSpan();

  try {
    const quote = await getRandomQuote();
    currentSpan?.addEvent("Quote Fetched Successfully", { quote_id: quote.id });

    const savedResponse = await saveQuote(quote);
    currentSpan?.addEvent("Quote Saved Successfully", { quote_id: quote.id });

    return {
      statusCode: 200,
      body: JSON.stringify({
        message: "Quote Processed Successfully",
        quote,
        savedResponse,
      }),
      headers: {
        "Content-Type": "application/json",
      },
    };
  } catch (error) {
    currentSpan?.recordException(error as Error);
    currentSpan?.setStatus({ code: SpanStatusCode.ERROR });

    return {
      statusCode: 500,
      body: JSON.stringify({
        message: "Error processing quote",
        error: (error as Error).message,
      }),
      headers: {
        "Content-Type": "application/json",
      },
    };
  }
}

// Extract attributes for EventBridge Scheduler events.
export function scheduledEventExtractor(
  event: unknown,
  context: LambdaContext,
) {
  const baseAttributes = defaultExtractor(event, context);

  return {
    kind: SpanKind.SERVER,
    ...baseAttributes,
    trigger: TriggerType.Timer,
    spanName: "generate-quotes",
  };
}

// Create the traced handler
const traced = createTracedHandler<ScheduledEvent>(
  "quotes-function",
  completionHandler,
  scheduledEventExtractor,
);

// The handler accepts ScheduledEvent inputs and uses the lambdaHandler function
export const handler = traced(lambdaHandler);

//==============================================================================
// HELPER FUNCTIONS
//==============================================================================

/**
 * Fetches a random quote from the external API and validates its structure.
 *
 * @returns A validated Quote object
 * @throws Error if the API request fails or if the response doesn't match the schema
 */
async function getRandomQuote(): Promise<Quote> {
  return tracer.startActiveSpan("get_random_quote", async (span) => {
    try {
      const response = await fetch(QUOTES_URL);

      span.setAttributes({
        "http.url": QUOTES_URL,
        "http.method": "GET",
        "http.status_code": response.status,
      });

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }

      const data = await response.json();
      return QuoteSchema.parse(data);
    } catch (error) {
      span.recordException(error as Error);
      span.setStatus({ code: SpanStatusCode.ERROR });
      throw error;
    } finally {
      span.end();
    }
  });
}

/**
 * Saves a quote to the target endpoint with proper telemetry tracking.
 *
 * @param quote - The quote object to save
 * @returns The response from the target endpoint
 * @throws Error if the save operation fails
 */
async function saveQuote(quote: Quote): Promise<unknown> {
  return tracer.startActiveSpan("save_quote", async (span) => {
    try {
      const response = await fetch(TARGET_URL, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(quote),
      });

      span.setAttributes({
        "http.url": TARGET_URL,
        "http.method": "POST",
        "http.status_code": response.status,
        "quote.id": quote.id,
      });

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }

      return await response.json();
    } catch (error) {
      span.recordException(error as Error);
      span.setStatus({ code: SpanStatusCode.ERROR });
      throw error;
    } finally {
      span.end();
    }
  });
}
