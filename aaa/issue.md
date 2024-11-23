### Describe the feature

AWS CDK should support the new [EventBridge Event Buses logging](https://aws.amazon.com/about-aws/whats-new/2025/07/amazon-eventbridge-enhanced-logging-improved-observability/) capability. This is similar to the [EventBridge Pipes logging](https://aws.amazon.com/about-aws/whats-new/2023/11/amazon-eventbridge-logging-improved-observability//) capability added a couple of years ago.


### Use Case

With the new EventBridge enhanced logging capability, developers can now monitor and debug event-driven applications with comprehensive logs that provide visibility into the complete event journey. This addresses microservices and event-driven architecture monitoring challenges by providing detailed event lifecycle tracking.

### Proposed Solution

Add logging configuration support to the `EventBus` construct in the `aws-events` module, following the proven design patterns from the EventBridge Pipes alpha module (`@aws-cdk/aws-pipes-alpha`) to ensure consistency across EventBridge services.

### Proposed API Design

```typescript
import * as events from 'aws-cdk-lib/aws-events';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as s3 from 'aws-cdk-lib/aws-s3';
import * as firehose from 'aws-cdk-lib/aws-kinesisfirehose';

// Create an event bus with logging configuration
const logGroup = new logs.LogGroup(this, 'EventBridgeLogGroup');

const eventBus = new events.EventBus(this, 'MyEventBus', {
  eventBusName: 'my-custom-bus',
  logDestinations: [
    new events.CloudwatchLogsLogDestination(logGroup)
  ],
  logLevel: events.LogLevel.INFO,
  logIncludeExecutionData: [events.IncludeExecutionData.ALL]
});

// Configure multiple log destinations
const s3Bucket = new s3.Bucket(this, 'LogBucket');
const deliveryStream = new firehose.DeliveryStream(this, 'LogStream', {
  destinations: [new firehose.destinations.S3Bucket(s3Bucket)]
});

eventBus.configureLogging({
  logDestinations: [
    new events.CloudwatchLogsLogDestination(logGroup),
    new events.S3LogDestination({
      bucket: s3Bucket,
      prefix: 'eventbridge-logs/',
      outputFormat: events.S3OutputFormat.JSON
    }),
    new events.FirehoseLogDestination(deliveryStream)
  ],
  logLevel: events.LogLevel.ERROR,
  logIncludeExecutionData: [events.IncludeExecutionData.ALL]
});
```

### Supported Features

**Log Destinations:**

- CloudWatch Logs
- Amazon S3  
- Amazon Data Firehose

**Log Levels:**

- `OFF` - Disable logging
- `ERROR` - Log only errors
- `INFO` - Log informational messages and errors
- `TRACE` - Log detailed trace information

**Include Execution Data Options:**

- `ALL` - Include detailed event information in logs (event data and target details)
- `NONE` - Include only basic logging information without detailed event data

**Configuration Options:**

- Multiple destination support
- Event-level logging control
- Privacy control for detailed event data inclusion
- Encryption support (customer-managed keys)

### Implementation Details

The CDK construct should map to the EventBridge logging configuration while providing a developer-friendly interface. EventBridge Event Bus logging supports the same destinations as EventBridge Pipes, following the proven design patterns from the EventBridge Pipes alpha module (`@aws-cdk/aws-pipes-alpha`) to ensure consistency across EventBridge services.

**Key patterns to adopt:**

1. `ILogDestination` interface with `bind()` and `grantPush()` methods
2. Separate destination classes (`CloudwatchLogsLogDestination`, `FirehoseLogDestination`, `S3LogDestination`)
3. Configuration enums (`LogLevel`, `IncludeExecutionData`, `S3OutputFormat`)
4. IAM permission management through `grantPush()` method
5. CloudFormation property mapping through `bind()` method

**Rationale:** EventBridge Event Buses and Pipes share identical logging requirements (same destinations, same service family), making the Pipes alpha module the logical foundation for consistent developer experience across EventBridge constructs.

**CloudFormation Mapping:** The implementation should generate the appropriate CloudFormation properties for the [`AWS::Events::EventBus LogConfig`](https://docs.aws.amazon.com/AWSCloudFormation/latest/TemplateReference/aws-properties-events-eventbus-logconfig.html) parameter while handling the complexity of multiple destinations and IAM permissions at the CDK level.

### Other Information

No response

### Acknowledgements

- [ ] I may be able to implement this feature request
- [ ] This feature might incur a breaking change

### AWS CDK Library version (aws-cdk-lib)

2.206.0

### AWS CDK CLI version

2.1021.0

### Environment details (OS name and version, etc.)

Ubuntu 24.04
