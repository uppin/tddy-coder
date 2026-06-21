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

// ---------------------------------------------------------------------------
// defaultRequestJson
// ---------------------------------------------------------------------------

describe("defaultRequestJson", () => {
  it("returns parseable proto-JSON with a 'message' field defaulting to empty string", () => {
    // When
    const json = defaultRequestJson(EchoRequestSchema);

    // Then
    expect(json).not.toBe("");
    const parsed = JSON.parse(json); // throws if not valid JSON
    expect(parsed).not.toBeNull();
    expect("message" in parsed).toBe(true);
    expect(parsed.message).toBe("");
  });
});

// ---------------------------------------------------------------------------
// parseRequestJson
// ---------------------------------------------------------------------------

describe("parseRequestJson", () => {
  it("parses valid JSON and returns the proto value without error", () => {
    // When
    const result = parseRequestJson(EchoRequestSchema, '{"message":"hello"}');

    // Then
    expect(result.error).toBeUndefined();
    expect(result.value).toBeDefined();
  });

  it("does not throw on empty string input", () => {
    // FIXME(implementer): once protoMessageJson.ts is written, narrow to either
    //   expect(result.error).toBeUndefined()   — if empty string yields a default proto value
    //   expect(typeof result.error).toBe("string")  — if empty string is treated as a parse error
    // The contract should be deterministic; the disjunction here documents the open design choice.
    expect(() => parseRequestJson(EchoRequestSchema, "")).not.toThrow();
  });

  it("surfaces a string error for invalid JSON without throwing", () => {
    // When
    const result = parseRequestJson(EchoRequestSchema, "{ invalid json {{");

    // Then
    expect(typeof result.error).toBe("string");
    expect(result.value).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// patchRequestJsonField
// ---------------------------------------------------------------------------

describe("patchRequestJsonField", () => {
  it("sets one field and re-normalizes through protobuf-es", () => {
    // Given
    const initial = defaultRequestJson(EchoRequestSchema);

    // When
    const patched = patchRequestJsonField(EchoRequestSchema, initial, "message", "hello world");

    // Then
    const parsed = JSON.parse(patched);
    expect(parsed.message).toBe("hello world");
  });

  it("overwrites the previous value on a repeat patch", () => {
    // Given
    const initial = defaultRequestJson(EchoRequestSchema);
    const once = patchRequestJsonField(EchoRequestSchema, initial, "message", "first");

    // When
    const twice = patchRequestJsonField(EchoRequestSchema, once, "message", "second");

    // Then
    const parsed = JSON.parse(twice);
    expect(parsed.message).toBe("second");
  });
});

// ---------------------------------------------------------------------------
// round-trip JSON ⇄ form
// ---------------------------------------------------------------------------

describe("round-trip JSON ⇄ form", () => {
  it("patching a field and reading it back returns the same value", () => {
    // Given
    const schema = EchoRequestSchema;
    const initial = defaultRequestJson(schema);
    const patched = patchRequestJsonField(schema, initial, "message", "round-trip-value");
    const field = schema.fields.find((f) => f.jsonName === "message" || f.name === "message");
    expect(field).toBeDefined();

    // When
    const readBack = readFieldForBuilder(schema, patched, field!);

    // Then
    expect(readBack).toBe("round-trip-value");
  });
});
