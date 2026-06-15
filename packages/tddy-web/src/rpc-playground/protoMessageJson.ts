/**
 * Helpers for editing protobuf request messages as JSON in the RPC Playground.
 *
 * Ported for @bufbuild/protobuf v2: scalar fields expose `field.scalar` (a `ScalarType`),
 * and field kinds are discriminated via `field.fieldKind`.
 */
import {
  type DescMessage,
  type DescField,
  type JsonValue,
  ScalarType,
  create,
  fromJson,
  fromJsonString,
  toJson,
  toJsonString,
} from "@bufbuild/protobuf";

/** Pretty-printed JSON for an empty (default) instance of `schema`. */
export function defaultRequestJson(schema: DescMessage): string {
  return toJsonString(schema, create(schema, {}), { prettySpaces: 2, alwaysEmitImplicit: true });
}

export type ParseResult =
  | { value: Record<string, unknown>; error?: undefined }
  | { value?: undefined; error: string };

/** Parse and normalize a JSON request against `schema`. Empty input is treated as `{}`. */
export function parseRequestJson(schema: DescMessage, json: string): ParseResult {
  const input = json.trim() === "" ? "{}" : json;
  try {
    const msg = fromJsonString(schema, input);
    const value = toJson(schema, msg) as Record<string, unknown>;
    return { value };
  } catch (e) {
    return { error: String(e) };
  }
}

/** Set a single field (by its JSON name) in the request JSON, re-normalizing if possible. */
export function patchRequestJsonField(
  schema: DescMessage,
  json: string,
  jsonName: string,
  value: unknown,
): string {
  const input = json.trim() === "" ? "{}" : json;
  let base: Record<string, unknown>;
  try {
    base = JSON.parse(input) as Record<string, unknown>;
  } catch {
    base = {};
  }
  const patched = { ...base, [jsonName]: value };
  try {
    const msg = fromJson(schema, patched as JsonValue);
    return toJsonString(schema, msg, { prettySpaces: 2 });
  } catch {
    return JSON.stringify(patched, null, 2);
  }
}

/** Read a field's current value from the request JSON for the form builder. */
export function readFieldForBuilder(
  schema: DescMessage,
  json: string,
  field: DescField,
): unknown {
  const result = parseRequestJson(schema, json);
  if (result.value === undefined) {
    return defaultFieldJsonValue(field);
  }
  return (
    result.value[field.jsonName] ??
    result.value[field.name] ??
    defaultFieldJsonValue(field)
  );
}

/** A sensible zero-value (in JSON form) for a field, used to seed the builder. */
export function defaultFieldJsonValue(field: DescField): unknown {
  switch (field.fieldKind) {
    case "scalar":
      switch (field.scalar) {
        case ScalarType.BOOL:
          return false;
        case ScalarType.STRING:
          return "";
        case ScalarType.BYTES:
          return "";
        default:
          return 0;
      }
    case "enum":
      return field.enum.values[0]?.name ?? "";
    case "message":
      return {};
    case "list":
      return [];
    case "map":
      return {};
    default:
      return null;
  }
}
