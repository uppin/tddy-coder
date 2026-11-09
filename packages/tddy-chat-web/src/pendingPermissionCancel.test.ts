/**
 * Unit tests — reading who cancelled a permission request, and who chose an option, from the answer
 * its transcript records.
 *
 * A cancelled request names the actor who cancelled it and no option; an option answer names the
 * option and the user who chose it. The entry words the two differently, so the reading keeps them
 * apart.
 */

import { describe, expect, it } from "bun:test";
import { answerFieldsOf } from "./pendingPermission";

describe("answerFieldsOf", () => {
  it("reads a cancellation as cancelled, naming who cancelled it", () => {
    const output = JSON.stringify({ answeredBy: "main-agent", cancelled: true });
    expect(answerFieldsOf(output)).toEqual({ answeredBy: "main-agent", answeredCancelled: true });
  });

  it("reads an option answer as not cancelled, naming the option and who chose it", () => {
    const output = JSON.stringify({ answeredBy: "alice", optionId: "allow-once" });
    expect(answerFieldsOf(output)).toEqual({
      answeredBy: "alice",
      answeredOptionId: "allow-once",
      answeredCancelled: false,
    });
  });

  it("reads nothing from an output that records no answer", () => {
    expect(answerFieldsOf(undefined)).toBeNull();
    expect(answerFieldsOf("main-agent")).toBeNull();
    expect(answerFieldsOf(JSON.stringify({ cancelled: true }))).toBeNull();
  });
});
