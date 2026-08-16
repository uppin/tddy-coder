/**
 * Unit tests — how an ACP tool call reads in a transcript.
 *
 * The live chat and the read-only replay both fold `tool_call` / `tool_call_update` frames into a
 * bubble, so this mapping is the one place either surface learns whether a call ran, failed, or is
 * still going, and what it produced.
 */

import { describe, expect, it } from "bun:test";
import { ToolCallStatus } from "../../gen/tddy/acp/v1/acp_pb";
import { toolEntryText, toolResultText, toolStatusOf } from "./toolCallPresentation";

describe("toolStatusOf", () => {
  it("reads a completed call as completed", () => {
    expect(toolStatusOf(ToolCallStatus.COMPLETED)).toEqual("completed");
  });

  it("reads a failed call as an error", () => {
    expect(toolStatusOf(ToolCallStatus.FAILED)).toEqual("error");
  });

  it("reads an in-progress call as running", () => {
    expect(toolStatusOf(ToolCallStatus.IN_PROGRESS)).toEqual("running");
  });

  it("reads a call that named no status as running rather than as finished", () => {
    expect(toolStatusOf(undefined)).toEqual("running");
  });
});

describe("toolResultText", () => {
  it("unwraps a JSON string into the tool's own prose", () => {
    // Given — how `tddy-acp::provider_agent` carries non-JSON tool output
    const rawOutput = JSON.stringify("3 matches in src/main.rs");

    // When / Then
    expect(toolResultText(rawOutput)).toEqual("3 matches in src/main.rs");
  });

  it("shows a JSON document as it arrived", () => {
    expect(toolResultText('{"matches":3}')).toEqual('{"matches":3}');
  });

  it("shows output that is not JSON at all as it arrived", () => {
    expect(toolResultText("error: no such file")).toEqual("error: no such file");
  });

  it("has no result for a call whose update carried none", () => {
    expect(toolResultText(undefined)).toEqual("");
  });

  it("has no result for whitespace-only output", () => {
    expect(toolResultText("   \n ")).toEqual("");
  });
});

describe("toolEntryText", () => {
  it("shows what was called and what it produced", () => {
    expect(toolEntryText("Grep", JSON.stringify("3 matches"))).toEqual("Grep\n3 matches");
  });

  it("shows only what was called while the result is still outstanding", () => {
    expect(toolEntryText("Grep", undefined)).toEqual("Grep");
  });
});
