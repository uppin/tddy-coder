/**
 * Unit tests for how the web renders a protobuf enum value it has no name for.
 *
 * Changeset: `CS-2026-08-16-models-and-assistants`
 */

import { describe, it, expect } from "bun:test";
import { unrecognisedEnumText } from "./enumSkew";

describe("unrecognisedEnumText", () => {
  it("names the enum and renders the value the daemon actually sent", () => {
    // Given / When
    const text = unrecognisedEnumText("provider kind", 7);

    // Then — an operator can quote the number back at the daemon, which a friendly word loses
    expect(text).toEqual(
      "Unrecognised provider kind 7 — the daemon sent a value this web build has no name for",
    );
  });

  it("renders the unset value as itself rather than as a named state", () => {
    // Given — a field the daemon left unset arrives as 0, indistinguishable from a value this
    // build predates
    // When / Then
    expect(unrecognisedEnumText("residency", 0)).toEqual(
      "Unrecognised residency 0 — the daemon sent a value this web build has no name for",
    );
  });
});
