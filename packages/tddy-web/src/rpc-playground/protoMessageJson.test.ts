/**
 * RPC Playground — synced JSON ⇄ form editor logic tests.
 *
 * Tests the single-JSON-string source-of-truth pattern ported from
 * ~/Code/meshstream/ots-meshcore-plugin/ui-src/src/dev/protoMessageJson.ts.
 * The canonical JSON string is always normalized through @bufbuild/protobuf so
 * round-trips between the builder view and raw JSON view are lossless.
 *
 * All imports will fail to resolve until protoMessageJson.ts is created (red phase).
 */

import { describe, expect, it } from "bun:test";
// These imports fail until protoMessageJson.ts is created.
import {
  defaultRequestJson,
  parseRequestJson,
  patchRequestJsonField,
  readFieldForBuilder,
} from "./protoMessageJson";

// EchoRequestSchema will come from the generated protobuf-es descriptor.
// Import fails until the reflection plumbing is in place and re-export is set up.
import { EchoRequestSchema } from "../gen/test/echo_service_pb";

describe("defaultRequestJson", () => {
  it("returns valid proto-JSON for EchoRequest", () => {
    const json = defaultRequestJson(EchoRequestSchema);
    const parsed = JSON.parse(json);
    // EchoRequest has a `message` string field — default is empty string.
    expect(typeof parsed).toBe("object");
    expect("message" in parsed).toBe(true);
    expect(parsed.message).toBe("");
  });

  it("returns a non-empty string", () => {
    const json = defaultRequestJson(EchoRequestSchema);
    expect(typeof json).toBe("string");
    expect(json.trim().length).toBeGreaterThan(0);
  });
});

describe("parseRequestJson", () => {
  it("parses valid JSON and returns a value", () => {
    const result = parseRequestJson(EchoRequestSchema, '{"message":"hello"}');
    expect(result.error).toBeUndefined();
    expect(result.value).toBeDefined();
  });

  it("treats empty string as empty object without throwing", () => {
    const result = parseRequestJson(EchoRequestSchema, "");
    // Must not throw; must return either a value or an error string.
    expect(result.error === undefined || typeof result.error === "string").toBe(true);
  });

  it("surfaces an error for invalid JSON without throwing", () => {
    const result = parseRequestJson(EchoRequestSchema, "{ invalid json {{");
    expect(typeof result.error).toBe("string");
    expect(result.value).toBeUndefined();
  });
});

describe("patchRequestJsonField", () => {
  it("sets one field and re-normalizes through protobuf-es", () => {
    const initial = defaultRequestJson(EchoRequestSchema);
    const patched = patchRequestJsonField(EchoRequestSchema, initial, "message", "hello world");
    const parsed = JSON.parse(patched);
    expect(parsed.message).toBe("hello world");
  });

  it("overwrites a previous value on repeat patch", () => {
    const initial = defaultRequestJson(EchoRequestSchema);
    const once = patchRequestJsonField(EchoRequestSchema, initial, "message", "first");
    const twice = patchRequestJsonField(EchoRequestSchema, once, "message", "second");
    const parsed = JSON.parse(twice);
    expect(parsed.message).toBe("second");
  });
});

describe("round-trip JSON ⇄ form", () => {
  it("patching a field and reading it back returns the same value", () => {
    const schema = EchoRequestSchema;
    const initial = defaultRequestJson(schema);
    const patched = patchRequestJsonField(schema, initial, "message", "round-trip-value");
    // Find the message field descriptor.
    const field = schema.fields.find((f) => f.jsonName === "message" || f.name === "message");
    expect(field).toBeDefined();
    const readBack = readFieldForBuilder(schema, patched, field!);
    expect(readBack).toBe("round-trip-value");
  });
});
