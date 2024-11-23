import { awscdk, javascript } from "projen";
import { IndentStyle } from "projen/lib/javascript/biome/biome-config";

const project = new awscdk.AwsCdkTypeScriptApp({
  name: "cdk-aws-serverless-otlp-forwarder-cwl",

  // Base
  defaultReleaseBranch: "main",
  depsUpgradeOptions: { workflow: false },
  gitignore: ["**/target"],
  projenrcTs: true,

  // Toolchain
  biome: true,
  biomeOptions: {
    biomeConfig: {
      assist: {
        enabled: true,
        actions: {
          source: {
            organizeImports: {
              level: "on",
              options: {
                identifierOrder: "lexicographic",
              },
            },
          },
        },
      },
      formatter: {
        enabled: true,
        indentStyle: IndentStyle.SPACE,
        indentWidth: 2,
        lineWidth: 100,
      },
      linter: {
        enabled: true,
        rules: {
          recommended: true,
          suspicious: {
            noShadowRestrictedNames: "off",
          },
        },
      },
    },
  },
  cdkVersion: "2.219.0",
  minNodeVersion: "22.20.0",
  packageManager: javascript.NodePackageManager.PNPM,
  pnpmVersion: "10",

  // Deps
  deps: [
    "@dev7a/lambda-otel-lite",
    "@opentelemetry/api",
    "@opentelemetry/core",
    "@opentelemetry/instrumentation",
    "@opentelemetry/instrumentation-aws-sdk",
    "@opentelemetry/instrumentation-http",
    "@opentelemetry/instrumentation-undici",
    "@opentelemetry/otlp-exporter-base",
    "@opentelemetry/resource-detector-aws",
    "@opentelemetry/resources",
    "@opentelemetry/sdk-trace-base",
    "@opentelemetry/sdk-trace-node",
    "@opentelemetry/semantic-conventions",
    "@types/aws-lambda",
    "cargo-lambda-cdk",
    "uv-python-lambda",
    "zod",
  ],
});

project.synth();
