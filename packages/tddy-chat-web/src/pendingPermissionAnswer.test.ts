/**
 * Unit tests — reading the answer a permission request's transcript records.
 *
 * The answered frame is the request's tool call once it completed, and its raw output is the
 * answer: `{answeredBy, optionId}` for a chosen option, `{answeredBy, cancelled: true}` for a
 * cancellation. Anything else is not an answer, so a request is read as unanswered, not guessed at.
 */

import { describe, expect, it } from "bun:test";
import { answeredRecordOf } from "./pendingPermission";

describe("answeredRecordOf", () => {
  it("reads the option a user chose and who chose it", () => {
    const output = JSON.stringify({ answeredBy: "alice", optionId: "allow-once" });
    expect(answeredRecordOf(output)).toEqual({ answeredBy: "alice", optionId: "allow-once" });
  });

  it("reads a cancellation as an answer with no option", () => {
    const output = JSON.stringify({ answeredBy: "alice", cancelled: true });
    expect(answeredRecordOf(output)).toEqual({ answeredBy: "alice", optionId: null });
  });

  it("reads nothing when no output was recorded", () => {
    expect(answeredRecordOf(undefined)).toBeNull();
  });

  it("reads nothing from an output that is not JSON", () => {
    expect(answeredRecordOf("allow-once")).toBeNull();
  });

  it("reads nothing when the answerer is not named", () => {
    expect(answeredRecordOf(JSON.stringify({ optionId: "allow-once" }))).toBeNull();
    expect(answeredRecordOf(JSON.stringify({ answeredBy: "", optionId: "allow-once" }))).toBeNull();
  });

  it("reads nothing when the answer names neither an option nor a cancellation", () => {
    expect(answeredRecordOf(JSON.stringify({ answeredBy: "alice" }))).toBeNull();
    expect(answeredRecordOf(JSON.stringify({ answeredBy: "alice", optionId: "" }))).toBeNull();
  });
});
