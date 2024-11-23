import json
import os
import requests

from opentelemetry import trace
from opentelemetry.trace import StatusCode

from lambda_otel_lite import init_telemetry, create_traced_handler

# Initialize telemetry once at module load
tracer, completion_handler = init_telemetry()

# Create a sessions object for requests
http_session = requests.Session()
target_url = os.environ.get("TARGET_URL")
quotes_url = "https://dummyjson.com/quotes/random"


@tracer.start_as_current_span("get_random_quote")
def get_random_quote():
    """Get a random quote from the API."""
    response = http_session.get(quotes_url)
    response.raise_for_status()
    return response.json()


@tracer.start_as_current_span("save_quote")
def save_quote(quote: dict):
    """Save the quote to the target URL."""
    response = http_session.post(
        f"{target_url}",
        json=quote,
        headers={
            "content-type": "application/json",
        },
    )
    response.raise_for_status()
    return response.json()


# Create a traced handler
traced = create_traced_handler(
    name="lambda-handler",
    completion_handler=completion_handler,
)


@traced
def handler(event, context):
    """Lambda handler function.

    This handler retrieves a random quote and saves it to the target URL.
    """
    current_span = trace.get_current_span()
    current_span.add_event(
        "Lambda Invocation Started",
        attributes={
            "event": json.dumps(event),
        },
    )

    try:
        quote = get_random_quote()
        response = save_quote(quote)

        current_span.add_event(
            "Quote Saved",
            attributes={
                "quote": quote["quote"],
            },
        )

        current_span.add_event("Lambda Execution Completed")

        return {
            "statusCode": 200,
            "body": json.dumps(
                {
                    "message": "Hello from Lambda!",
                    "input": event,
                    "quote": quote,
                    "response": response,
                }
            ),
        }
    except Exception as e:
        current_span.record_exception(e)
        current_span.set_status(StatusCode.ERROR, str(e))
        raise
