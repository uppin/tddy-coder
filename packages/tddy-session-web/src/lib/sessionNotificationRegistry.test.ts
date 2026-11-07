import { describe, expect, it } from "bun:test";
import { SessionNotificationRegistry } from "./sessionNotificationRegistry";

/**
 * The per-session store behind the drawer's indicators.
 *
 * PRD: docs/ft/daemon/session-notifications.md (FR4–FR6).
 *
 * It holds three moments per session — last activity, last attention, last view — and nothing
 * else: the indicator itself is derived (`lib/sessionIndicator.ts`), so the store's only jobs are
 * keeping sessions apart, keeping the newest timestamp, and telling `useSyncExternalStore`
 * subscribers when something actually changed. It mirrors `agentActivityRegistry`, which is the
 * established shape for this in the app.
 */

const SESSION_A = "indicator-aaaaaaaa-0000-0000-0000-000000000001";
const SESSION_B = "indicator-bbbbbbbb-0000-0000-0000-000000000002";
const NOW = 1_756_000_000_000;

describe("SessionNotificationRegistry — what each session last did", () => {
  // -------------------------------------------------------------------------
  // Recording
  // -------------------------------------------------------------------------

  it("has nothing recorded for a session it has never heard of", () => {
    // Given
    const registry = new SessionNotificationRegistry();

    // When
    const state = registry.get(SESSION_A);

    // Then
    expect(state).toBeUndefined();
  });

  it("records the moment a session last reported activity", () => {
    // Given
    const registry = new SessionNotificationRegistry();

    // When
    registry.recordActivity(SESSION_A, NOW);

    // Then
    expect(registry.get(SESSION_A)).toEqual({
      lastActivityAtMs: NOW,
      attentionAtMs: 0,
      seenAtMs: 0,
    });
  });

  it("records the moment a session last asked for attention", () => {
    // Given
    const registry = new SessionNotificationRegistry();

    // When
    registry.recordAttention(SESSION_A, NOW);

    // Then
    expect(registry.get(SESSION_A)).toEqual({
      lastActivityAtMs: 0,
      attentionAtMs: NOW,
      seenAtMs: 0,
    });
  });

  it("records activity and attention side by side for one session", () => {
    // Given
    const registry = new SessionNotificationRegistry();

    // When
    registry.recordActivity(SESSION_A, NOW - 1_000);
    registry.recordAttention(SESSION_A, NOW);

    // Then — attention must not erase the activity that preceded it
    expect(registry.get(SESSION_A)).toEqual({
      lastActivityAtMs: NOW - 1_000,
      attentionAtMs: NOW,
      seenAtMs: 0,
    });
  });

  it("marks a session seen at the given moment", () => {
    // Given
    const registry = new SessionNotificationRegistry();
    registry.recordAttention(SESSION_A, NOW - 5_000);

    // When
    registry.markSeen(SESSION_A, NOW);

    // Then
    expect(registry.get(SESSION_A)).toEqual({
      lastActivityAtMs: 0,
      attentionAtMs: NOW - 5_000,
      seenAtMs: NOW,
    });
  });

  it("marks a session seen even when the feed has never mentioned it", () => {
    // Given — the operator opens a quiet session
    const registry = new SessionNotificationRegistry();

    // When
    registry.markSeen(SESSION_A, NOW);

    // Then
    expect(registry.get(SESSION_A)).toEqual({
      lastActivityAtMs: 0,
      attentionAtMs: 0,
      seenAtMs: NOW,
    });
  });

  // -------------------------------------------------------------------------
  // Newest wins
  // -------------------------------------------------------------------------

  it("keeps the newest activity timestamp when an older one arrives out of order", () => {
    // Given — a reconnect can replay an event behind one already applied
    const registry = new SessionNotificationRegistry();
    registry.recordActivity(SESSION_A, NOW);

    // When
    registry.recordActivity(SESSION_A, NOW - 10_000);

    // Then
    expect(registry.get(SESSION_A)?.lastActivityAtMs).toBe(NOW);
  });

  it("keeps the newest attention timestamp when an older one arrives out of order", () => {
    // Given
    const registry = new SessionNotificationRegistry();
    registry.recordAttention(SESSION_A, NOW);

    // When
    registry.recordAttention(SESSION_A, NOW - 10_000);

    // Then
    expect(registry.get(SESSION_A)?.attentionAtMs).toBe(NOW);
  });

  it("keeps the newest view when an older one arrives out of order", () => {
    // Given
    const registry = new SessionNotificationRegistry();
    registry.markSeen(SESSION_A, NOW);

    // When
    registry.markSeen(SESSION_A, NOW - 10_000);

    // Then
    expect(registry.get(SESSION_A)?.seenAtMs).toBe(NOW);
  });

  // -------------------------------------------------------------------------
  // Per-session isolation
  // -------------------------------------------------------------------------

  it("keeps each session's notifications apart", () => {
    // Given
    const registry = new SessionNotificationRegistry();

    // When
    registry.recordActivity(SESSION_A, NOW);
    registry.recordAttention(SESSION_B, NOW - 1_000);

    // Then
    expect(registry.get(SESSION_A)).toEqual({
      lastActivityAtMs: NOW,
      attentionAtMs: 0,
      seenAtMs: 0,
    });
    expect(registry.get(SESSION_B)).toEqual({
      lastActivityAtMs: 0,
      attentionAtMs: NOW - 1_000,
      seenAtMs: 0,
    });
  });

  it("leaves other sessions untouched when one is marked seen", () => {
    // Given
    const registry = new SessionNotificationRegistry();
    registry.recordAttention(SESSION_A, NOW);
    registry.recordAttention(SESSION_B, NOW);

    // When
    registry.markSeen(SESSION_A, NOW + 1_000);

    // Then
    expect(registry.get(SESSION_B)?.seenAtMs).toBe(0);
  });

  // -------------------------------------------------------------------------
  // Subscriptions — the `useSyncExternalStore` contract
  // -------------------------------------------------------------------------

  it("notifies subscribers when a notification lands", () => {
    // Given
    const registry = new SessionNotificationRegistry();
    let notified = 0;
    registry.subscribe(() => {
      notified += 1;
    });

    // When
    registry.recordActivity(SESSION_A, NOW);

    // Then
    expect(notified).toBe(1);
  });

  it("does not notify when a stale timestamp changes nothing", () => {
    // Given
    const registry = new SessionNotificationRegistry();
    registry.recordActivity(SESSION_A, NOW);
    let notified = 0;
    registry.subscribe(() => {
      notified += 1;
    });

    // When — an older event for the same session leaves the state as it was
    registry.recordActivity(SESSION_A, NOW - 1_000);

    // Then — a needless notify re-renders every row in the drawer
    expect(notified).toBe(0);
  });

  it("returns a reference-stable state across a write to another session", () => {
    // Given — `useSyncExternalStore` tears if a snapshot changes identity without changing value
    const registry = new SessionNotificationRegistry();
    registry.recordActivity(SESSION_A, NOW);
    const before = registry.get(SESSION_A);

    // When
    registry.recordActivity(SESSION_B, NOW);

    // Then
    expect(registry.get(SESSION_A)).toBe(before);
  });

  it("stops notifying a subscriber that has unsubscribed", () => {
    // Given
    const registry = new SessionNotificationRegistry();
    let notified = 0;
    const unsubscribe = registry.subscribe(() => {
      notified += 1;
    });
    unsubscribe();

    // When
    registry.recordActivity(SESSION_A, NOW);

    // Then
    expect(notified).toBe(0);
  });

  // -------------------------------------------------------------------------
  // Reset — test isolation, as `agentActivityRegistry.reset()` provides
  // -------------------------------------------------------------------------

  it("forgets every session on reset", () => {
    // Given
    const registry = new SessionNotificationRegistry();
    registry.recordActivity(SESSION_A, NOW);
    registry.recordAttention(SESSION_B, NOW);

    // When
    registry.reset();

    // Then
    expect(registry.get(SESSION_A)).toBeUndefined();
    expect(registry.get(SESSION_B)).toBeUndefined();
  });

  it("does not notify on a reset that clears nothing", () => {
    // Given
    const registry = new SessionNotificationRegistry();
    let notified = 0;
    registry.subscribe(() => {
      notified += 1;
    });

    // When
    registry.reset();

    // Then
    expect(notified).toBe(0);
  });
});
