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

/**
 * The loaded range: which slice of the transcript is in memory, where it starts, and whether
 * anything older remains. The rendered consequences are pinned by
 * `cypress/component/ActivitiesTailScrollAcceptance.cy.tsx`; these specs pin the store contract the
 * paging depends on — a prepend that lands out of order, or a cursor that fails to advance, would
 * page the same frames in forever.
 */
describe("AgentActivityRegistry loaded range", () => {
  /** A transcript entry labelled by its position, so a prepend's ordering is readable. */
  function anEntry(label: string, at: number): ChatMessage {
    return { key: `agent-${label}`, text: label, from: "agent", at };
  }

  /** A session holding the newest page: entries 151 → 152, starting at seq 150, more behind it. */
  function aSessionOnItsNewestPage(): AgentActivityRegistry {
    const registry = new AgentActivityRegistry();
    registry.setMessages("s1", [anEntry("Entry 151", 151_000), anEntry("Entry 152", 152_000)]);
    registry.setOldestSeq("s1", 150, false);
    return registry;
  }

  it("prepends an older page above the loaded entries", () => {
    // Given — a session showing its newest page
    const registry = aSessionOnItsNewestPage();

    // When — the page behind it resolves
    registry.prependMessages(
      "s1",
      [anEntry("Entry 149", 149_000), anEntry("Entry 150", 150_000)],
      148,
      false,
    );

    // Then — the older entries lead the range, in their own recorded order
    expect(registry.get("s1")?.messages.map((entry) => entry.text)).toEqual([
      "Entry 149",
      "Entry 150",
      "Entry 151",
      "Entry 152",
    ]);
  });

  it("advances the reverse cursor to the prepended page's first seq", () => {
    // Given — a range starting at seq 150
    const registry = aSessionOnItsNewestPage();

    // When — two older entries land at seq 148
    registry.prependMessages(
      "s1",
      [anEntry("Entry 149", 149_000), anEntry("Entry 150", 150_000)],
      148,
      false,
    );

    // Then — the next fetch pages back from there, not from the old cursor
    expect(registry.get("s1")?.oldestSeq).toBe(148);
  });

  it("marks the range closed when the prepended page reaches the head", () => {
    // Given — a range one page from the transcript head
    const registry = aSessionOnItsNewestPage();

    // When — the page that resolves reaches it
    registry.prependMessages("s1", [anEntry("Entry 1", 1_000)], 0, true);

    // Then — the range is closed, so no further scroll issues a fetch
    expect(registry.get("s1")?.atOldest).toBe(true);
  });

  it("ignores a page whose first seq is not older than the current cursor", () => {
    // Given — a range that has already paged back to seq 148
    const registry = aSessionOnItsNewestPage();
    registry.prependMessages(
      "s1",
      [anEntry("Entry 149", 149_000), anEntry("Entry 150", 150_000)],
      148,
      false,
    );

    // When — a duplicate in-flight response for the page already prepended arrives
    registry.prependMessages(
      "s1",
      [anEntry("Entry 149", 149_000), anEntry("Entry 150", 150_000)],
      148,
      false,
    );

    // Then — it is dropped rather than double-rendered: the cursor is the arithmetic that makes a
    // duplicate detectable at all
    expect(registry.get("s1")?.messages.map((entry) => entry.text)).toEqual([
      "Entry 149",
      "Entry 150",
      "Entry 151",
      "Entry 152",
    ]);
  });
});
