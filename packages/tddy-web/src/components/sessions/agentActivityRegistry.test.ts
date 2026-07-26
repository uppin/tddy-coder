/**
 * Unit tests for the `AgentActivityRegistry` **tool-call body cache** — the per-session,
 * per-`callId` store that backs the detail dialog's lazy `GetAcpToolCallDetail` lookup. A fetched
 * body is cached so re-opening the same tool row reads it instead of re-fetching.
 *
 * Changeset: docs/dev/1-WIP/acp-replay-lazy-tool-bodies-web.md
 * Feature: docs/ft/web/agent-activity-pane.md § 4 Lazy tool bodies — fetch on click.
 */

import { describe, it, expect } from "bun:test";
import {
  AgentActivityRegistry,
  type ToolCallBodyState,
} from "./agentActivityRegistry";

const LOADED: ToolCallBodyState = {
  status: "loaded",
  rawInput: '{"command":"cargo test"}',
  rawOutput: '{"exit_code":0}',
};

describe("AgentActivityRegistry tool-call body cache", () => {
  it("returns no body for a call that has never been fetched", () => {
    // Given — a fresh registry
    const registry = new AgentActivityRegistry();

    // When / Then — an unseen (session, call) has no cached body
    expect(registry.getBody("s1", "tool-1")).toBeUndefined();
  });

  it("stores and returns a body keyed by session and call id", () => {
    // Given
    const registry = new AgentActivityRegistry();

    // When — a fetched body is cached
    registry.setBody("s1", "tool-1", LOADED);

    // Then — it reads back for that exact (session, call)
    expect(registry.getBody("s1", "tool-1")).toEqual(LOADED);
  });

  it("keeps distinct calls within one session separate", () => {
    // Given — two calls in the same session
    const registry = new AgentActivityRegistry();
    registry.setBody("s1", "tool-1", { status: "loading" });
    registry.setBody("s1", "tool-2", LOADED);

    // Then — each reads its own state
    expect(registry.getBody("s1", "tool-1")).toEqual({ status: "loading" });
    expect(registry.getBody("s1", "tool-2")).toEqual(LOADED);
  });

  it("keeps the same call id in different sessions separate", () => {
    // Given — the same call id fetched under two sessions
    const registry = new AgentActivityRegistry();
    registry.setBody("s1", "tool-1", LOADED);
    registry.setBody("s2", "tool-1", { status: "error", error: "not found" });

    // Then — the sessions do not collide
    expect(registry.getBody("s1", "tool-1")).toEqual(LOADED);
    expect(registry.getBody("s2", "tool-1")).toEqual({ status: "error", error: "not found" });
  });

  it("notifies subscribers when a body is written", () => {
    // Given — a subscribed registry
    const registry = new AgentActivityRegistry();
    let notifications = 0;
    registry.subscribe(() => {
      notifications += 1;
    });

    // When — a body is cached
    registry.setBody("s1", "tool-1", LOADED);

    // Then — the subscriber was notified once
    expect(notifications).toBe(1);
  });

  it("returns a reference-stable body across writes to other calls", () => {
    // Given — a cached body for tool-1
    const registry = new AgentActivityRegistry();
    registry.setBody("s1", "tool-1", LOADED);
    const first = registry.getBody("s1", "tool-1");

    // When — an unrelated call's body is written
    registry.setBody("s1", "tool-2", { status: "loading" });

    // Then — tool-1's cached body is the same reference (safe for useSyncExternalStore)
    expect(registry.getBody("s1", "tool-1")).toBe(first);
  });

  it("does not churn the transcript state when only a body is written", () => {
    // Given — a session whose transcript has been populated
    const registry = new AgentActivityRegistry();
    registry.setMessages("s1", [{ key: "a", text: "hi", from: "agent", at: 1 }]);
    const before = registry.get("s1");

    // When — a tool body is cached for that session
    registry.setBody("s1", "tool-1", LOADED);

    // Then — the transcript state object is untouched (the body cache is stored separately)
    expect(registry.get("s1")).toBe(before);
  });
});
