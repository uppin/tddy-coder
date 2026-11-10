/**
 * Unit tests — `isFrameForTerminal` (per-frame terminal identity guard).
 *
 * `SessionTerminalOutput` frames are stamped by the daemon with the session and terminal they came
 * from. A pane compares that stamp against the terminal it is rendering and drops anything foreign,
 * so a mis-routed subscription anywhere in the chain is caught at the write boundary instead of being
 * silently painted into the wrong terminal.
 *
 * The asymmetry that matters: a pane rendering the Agent terminal passes `terminalId: ""` in its
 * request (the daemon resolves empty to the reserved `main`), but every frame comes back stamped with
 * the RESOLVED id. The guard must resolve the pane's side the same way, or an Agent pane would reject
 * all of its own output.
 */

import { describe, expect, it } from "bun:test";
import { isFrameForTerminal } from "./terminalFrameIdentity";

/** A frame as stamped by the daemon — the terminal id is always the resolved one. */
function aFrameFrom(sessionId: string, terminalId: string) {
  return { sessionId, terminalId };
}

describe("isFrameForTerminal", () => {
  it("accepts a frame stamped with the pane's own session and terminal", () => {
    // Given — a pane rendering the bash-1 terminal of session 019fb141
    const pane = { sessionId: "019fb141", terminalId: "bash-1" };

    // When
    const accepted = isFrameForTerminal(aFrameFrom("019fb141", "bash-1"), pane);

    // Then
    expect(accepted).toBe(true);
  });

  it("accepts a main-stamped frame for an Agent pane that requested the empty terminal id", () => {
    // Given — the Agent pane passes "" in its request; the daemon stamps the resolved "main"
    const agentPane = { sessionId: "019fb141", terminalId: "" };

    // When
    const accepted = isFrameForTerminal(aFrameFrom("019fb141", "main"), agentPane);

    // Then
    expect(accepted).toBe(true);
  });

  it("rejects a frame stamped with a different session", () => {
    // Given — a pane rendering session 019fb141, and a frame belonging to 019fb136
    const pane = { sessionId: "019fb141", terminalId: "main" };

    // When
    const accepted = isFrameForTerminal(aFrameFrom("019fb136", "main"), pane);

    // Then — this is the cross-session bleed: another session's bytes must never be painted here
    expect(accepted).toBe(false);
  });

  it("rejects a frame from another terminal of the same session", () => {
    // Given — the Agent pane of a session that also has a bash tab open
    const agentPane = { sessionId: "019fb141", terminalId: "main" };

    // When
    const accepted = isFrameForTerminal(aFrameFrom("019fb141", "bash-1"), agentPane);

    // Then
    expect(accepted).toBe(false);
  });

  it("rejects an unstamped frame", () => {
    // Given — a pane rendering session 019fb141
    const pane = { sessionId: "019fb141", terminalId: "main" };

    // When — a frame arrives carrying no identity at all
    const accepted = isFrameForTerminal(aFrameFrom("", ""), pane);

    // Then — an unidentifiable frame is treated as foreign rather than trusted by default
    expect(accepted).toBe(false);
  });
});
