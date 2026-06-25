/**
 * Unit tests for the `toolSchema` JSON Schema → default-args skeleton helper.
 *
 * These are pure unit tests (no DOM, no React) run by Vitest.
 *
 * Changeset: `session-inspector-tools-tab`
 */

import { describe, it, expect } from "bun:test";
import { defaultArgsFromSchema } from "./toolSchema";

// ---------------------------------------------------------------------------
// Fixtures — JSON Schema strings (as returned by ListExecTools)
// ---------------------------------------------------------------------------

/** Read tool: path (required string). */
const READ_SCHEMA = JSON.stringify({
  type: "object",
  properties: {
    path: { type: "string" },
    limit: { type: "integer" },
    offset: { type: "integer" },
  },
  required: ["path"],
});

/** Shell tool: command (required string), env (optional object). */
const SHELL_SCHEMA = JSON.stringify({
  type: "object",
  properties: {
    command: { type: "string" },
    timeout: { type: "integer" },
    env: { type: "object", properties: {} },
  },
  required: ["command"],
});

/** Write tool: path and content (both required). */
const WRITE_SCHEMA = JSON.stringify({
  type: "object",
  properties: {
    path: { type: "string" },
    content: { type: "string" },
    create_dirs: { type: "boolean" },
  },
  required: ["path", "content"],
});

/** Grep tool: required array + required string. */
const GREP_SCHEMA = JSON.stringify({
  type: "object",
  properties: {
    pattern: { type: "string" },
    path: { type: "string" },
    include: { type: "array", items: { type: "string" } },
    flags: { type: "string" },
  },
  required: ["pattern", "path"],
});

/** Schema with a property that has an explicit `default` value. */
const DEFAULT_VALUE_SCHEMA = JSON.stringify({
  type: "object",
  properties: {
    timeout_ms: { type: "integer", default: 30000 },
  },
  required: ["timeout_ms"],
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("defaultArgsFromSchema", () => {
  it("returns a valid JSON string containing only required properties for Read schema", () => {
    // Given / When
    const result = defaultArgsFromSchema(READ_SCHEMA);

    // Then
    const parsed = JSON.parse(result) as Record<string, unknown>;
    expect(Object.keys(parsed)).toEqual(["path"]);
    expect(parsed["path"]).toBe("");
    // Optional properties must NOT be present
    expect("limit" in parsed).toBe(false);
    expect("offset" in parsed).toBe(false);
  });

  it("produces an empty string default for a required string property", () => {
    const result = defaultArgsFromSchema(SHELL_SCHEMA);
    const parsed = JSON.parse(result) as Record<string, unknown>;
    expect(parsed["command"]).toBe("");
    expect("timeout" in parsed).toBe(false);
    expect("env" in parsed).toBe(false);
  });

  it("includes all required properties when multiple are required", () => {
    const result = defaultArgsFromSchema(WRITE_SCHEMA);
    const parsed = JSON.parse(result) as Record<string, unknown>;
    expect(Object.keys(parsed).sort()).toEqual(["content", "path"]);
    expect(parsed["path"]).toBe("");
    expect(parsed["content"]).toBe("");
    expect("create_dirs" in parsed).toBe(false);
  });

  it("produces an empty array default for a required array property", () => {
    // Grep has pattern + path required; no array required — but let's test a schema
    // that has a required array.
    const schema = JSON.stringify({
      type: "object",
      properties: { items: { type: "array", items: { type: "string" } } },
      required: ["items"],
    });
    const result = defaultArgsFromSchema(schema);
    const parsed = JSON.parse(result) as Record<string, unknown>;
    expect(parsed["items"]).toEqual([]);
  });

  it("uses the schema's explicit default value when provided", () => {
    const result = defaultArgsFromSchema(DEFAULT_VALUE_SCHEMA);
    const parsed = JSON.parse(result) as Record<string, unknown>;
    expect(parsed["timeout_ms"]).toBe(30000);
  });

  it("returns '{}' for an empty or missing properties schema", () => {
    expect(defaultArgsFromSchema(JSON.stringify({ type: "object" }))).toBe("{}");
    expect(defaultArgsFromSchema(JSON.stringify({}))).toBe("{}");
  });

  it("returns '{}' for invalid JSON input", () => {
    expect(defaultArgsFromSchema("not-json")).toBe("{}");
    expect(defaultArgsFromSchema("")).toBe("{}");
  });

  it("returns '{}' when required is empty (no required properties)", () => {
    const result = defaultArgsFromSchema(GREP_SCHEMA.replace(
      '"required":["pattern","path"]',
      '"required":[]',
    ));
    const parsed = JSON.parse(result) as Record<string, unknown>;
    expect(Object.keys(parsed)).toHaveLength(0);
  });

  it("produces valid pretty-printed JSON that the user can edit in the textarea", () => {
    const result = defaultArgsFromSchema(READ_SCHEMA);
    // Must round-trip through JSON.parse without throwing.
    expect(() => JSON.parse(result)).not.toThrow();
    // Must be indented (pretty-printed) — contains newlines.
    expect(result).toContain("\n");
  });
});
