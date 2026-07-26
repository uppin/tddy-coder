/**
 * Unit coverage for the `AgentActivityRegistry` tool-body cache that backs the detail dialog's
 * fetch-on-open lookup. The dialog's observable behaviour (a reopened settled call issues no second
 * request; a running call re-requests) is pinned by
 * `cypress/component/AgentActivityDetailLazyFetchAcceptance.cy.tsx`; these specs pin the store
 * contract underneath it — keying, per-session isolation, snapshot immutability, and notification —
 * which the component specs can only observe indirectly.
 *
 * Each spec builds its own registry instance rather than the module singleton, so no case can leak
 * cached state into another.
 *
 * Feature doc: docs/ft/web/agent-activity-pane.md#rendering-an-unanswered-lookup-updated-2026-07-26
 */

import { describe, expect, it } from "bun:test";
import { AgentActivityRegistry } from "./agentActivityRegistry";
import type { ChatMessage } from "../chat/useAgentChat";

/** One tool call's bodies, as `GetAcpToolCallDetail` resolves them. */
function aToolDetail(command: string) {
  return { rawInput: JSON.stringify({ command }), rawOutput: JSON.stringify({ exit_code: 0 }) };
}

/** A transcript entry for the one case that asserts caching a body leaves the transcript alone. */
function aToolMessage(toolCallId: string): ChatMessage {
  return { key: "tool-0", text: "Bash cargo test", from: "tool", at: 1_000, toolCallId };
}

describe("AgentActivityRegistry tool-body cache", () => {
  it("returns no cached detail for a tool call that has not been fetched", () => {
    // Given — a session with streamed activity but no body lookup yet
    const registry = new AgentActivityRegistry();
    registry.setCount("s1", 3);

    // When
    const cached = registry.get("s1")?.toolDetails.get("tool-1");

    // Then — absence is reported as absence, never as an empty body
    expect(cached).toBeUndefined();
  });

  it("caches a fetched tool detail under its tool call id", () => {
    // Given
    const registry = new AgentActivityRegistry();

    // When
    registry.setToolDetail("s1", "tool-1", aToolDetail("cargo test --workspace"));

    // Then
    expect(registry.get("s1")?.toolDetails.get("tool-1")).toEqual({
      rawInput: '{"command":"cargo test --workspace"}',
      rawOutput: '{"exit_code":0}',
    });
  });

  it("keeps tool details of different calls in the same session independent", () => {
    // Given — one session, two distinct calls
    const registry = new AgentActivityRegistry();
    registry.setToolDetail("s1", "tool-1", aToolDetail("cargo test"));

    // When — a second call's bodies are cached
    registry.setToolDetail("s1", "tool-2", aToolDetail("cargo build"));

    // Then — the first call's bodies survive alongside the second
    expect(registry.get("s1")?.toolDetails.get("tool-1")?.rawInput).toBe('{"command":"cargo test"}');
    expect(registry.get("s1")?.toolDetails.get("tool-2")?.rawInput).toBe(
      '{"command":"cargo build"}',
    );
  });

  it("keeps tool details separate per session", () => {
    // Given — two sessions whose transcripts happen to share a tool call id
    const registry = new AgentActivityRegistry();
    registry.setToolDetail("s1", "tool-1", aToolDetail("cargo test"));

    // When
    registry.setToolDetail("s2", "tool-1", aToolDetail("cargo build"));

    // Then — one session's cache never answers for another's identically-named call
    expect(registry.get("s1")?.toolDetails.get("tool-1")?.rawInput).toBe('{"command":"cargo test"}');
    expect(registry.get("s2")?.toolDetails.get("tool-1")?.rawInput).toBe(
      '{"command":"cargo build"}',
    );
  });

  it("preserves the existing transcript and count when a tool detail is cached", () => {
    // Given — a session with a pulled snapshot and a streamed count
    const registry = new AgentActivityRegistry();
    const messages = [aToolMessage("tool-1")];
    registry.setCount("s1", 7);
    registry.setMessages("s1", messages);
    registry.markSnapshotLoaded("s1");

    // When — a body lookup resolves
    registry.setToolDetail("s1", "tool-1", aToolDetail("cargo test"));

    // Then — caching a body touches nothing else the overlay depends on
    const state = registry.get("s1");
    expect(state?.count).toBe(7);
    expect(state?.messages).toBe(messages);
    expect(state?.snapshotLoaded).toBe(true);
  });

  it("notifies subscribers when a tool detail is cached", () => {
    // Given — a subscriber, as `useSyncExternalStore` installs
    const registry = new AgentActivityRegistry();
    let notifications = 0;
    registry.subscribe(() => {
      notifications += 1;
    });

    // When
    registry.setToolDetail("s1", "tool-1", aToolDetail("cargo test"));

    // Then — the dialog re-reads the store and renders the cached body
    expect(notifications).toBe(1);
  });

  it("replaces the tool-detail map instead of mutating the previous snapshot", () => {
    // Given — a cached body, and the state snapshot observed at that moment
    const registry = new AgentActivityRegistry();
    registry.setToolDetail("s1", "tool-1", aToolDetail("cargo test"));
    const before = registry.get("s1");

    // When — a second body is cached
    registry.setToolDetail("s1", "tool-2", aToolDetail("cargo build"));

    // Then — the earlier snapshot is untouched, so `useSyncExternalStore` sees a real change
    expect(before?.toolDetails.has("tool-2")).toBe(false);
    expect(registry.get("s1")).not.toBe(before);
  });

  it("does not notify when the identical bodies are already cached", () => {
    // Given — a cached body and a subscriber installed afterwards
    const registry = new AgentActivityRegistry();
    registry.setToolDetail("s1", "tool-1", aToolDetail("cargo test"));
    let notifications = 0;
    registry.subscribe(() => {
      notifications += 1;
    });

    // When — the same call resolves again with the same bodies
    registry.setToolDetail("s1", "tool-1", aToolDetail("cargo test"));

    // Then — no re-render is triggered for an unchanged cache
    expect(notifications).toBe(0);
  });
});
