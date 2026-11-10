/**
 * JSON Schema helpers for the Tools tab invoke panel.
 *
 * The ExecuteTool catalog exposes each tool's input format as a `input_schema_json`
 * string (a JSON Schema object). This module provides utilities to convert that
 * schema into a JSON skeleton suitable for pre-filling the args textarea.
 *
 * NOTE: This is deliberately separate from the RPC Playground's `protoMessageJson.ts`
 * which operates on protobuf descriptors. Tool schemas are plain JSON Schema objects.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/** Simplified JSON Schema node (only the fields we inspect). */
interface JsonSchemaNode {
  type?: string | string[];
  properties?: Record<string, JsonSchemaNode>;
  required?: string[];
  items?: JsonSchemaNode;
  default?: unknown;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Build a JSON skeleton from a JSON Schema string.
 *
 * - Required properties are included with zero/empty default values.
 * - Optional properties are omitted.
 * - Unknown / missing schema → returns `"{}"`.
 *
 * @param inputSchemaJson - The raw `input_schema_json` string from a `ToolDef`.
 * @returns A pretty-printed JSON string suitable for the args textarea.
 */
export function defaultArgsFromSchema(inputSchemaJson: string): string {
  let schema: JsonSchemaNode;
  try {
    schema = JSON.parse(inputSchemaJson) as JsonSchemaNode;
  } catch {
    return "{}";
  }
  if (typeof schema !== "object" || schema === null) {
    return "{}";
  }
  const skeleton = buildSkeleton(schema);
  try {
    return JSON.stringify(skeleton, null, 2);
  } catch {
    return "{}";
  }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function buildSkeleton(schema: JsonSchemaNode): Record<string, unknown> {
  if (
    typeof schema.properties !== "object" ||
    schema.properties === null
  ) {
    return {};
  }
  const required = new Set<string>(Array.isArray(schema.required) ? schema.required : []);
  const result: Record<string, unknown> = {};
  for (const [key, propSchema] of Object.entries(schema.properties)) {
    if (!required.has(key)) continue;
    result[key] = defaultValueForSchema(propSchema);
  }
  return result;
}

function defaultValueForSchema(schema: JsonSchemaNode): unknown {
  if (schema.default !== undefined) {
    return schema.default;
  }
  const type = Array.isArray(schema.type)
    ? schema.type.find((t) => t !== "null") ?? schema.type[0]
    : schema.type;
  switch (type) {
    case "string":
      return "";
    case "number":
    case "integer":
      return 0;
    case "boolean":
      return false;
    case "array":
      return [];
    case "object":
      return buildSkeleton(schema);
    default:
      return null;
  }
}
