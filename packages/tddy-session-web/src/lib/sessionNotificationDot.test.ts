import { describe, expect, it } from "bun:test";
import {
  SessionNotificationKind,
  SessionNotificationSource,
} from "tddy-client-web/gen/activity_pb";
import { dotForNotification } from "./sessionNotificationDot";

/**
 * Which drawer dot a session notification lights.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-11-16-session-ended-notice.md — a session's end is attention
 * for an orchestrator's inbox, and lights nothing in the drawer.
 */

describe("dotForNotification — which dot an event lights", () => {
  it("a session end lights no dot", () => {
    // Given a session-ended event, which the daemon raises as attention
    const event = {
      kind: SessionNotificationKind.ATTENTION_REQUIRED,
      source: SessionNotificationSource.SESSION_ENDED,
    };

    // When / Then
    expect(dotForNotification(event)).toBe(null);
  });

  it("attention and activity light as before", () => {
    // Given an attention event from the hooks, and an activity event from a tool call
    const attention = {
      kind: SessionNotificationKind.ATTENTION_REQUIRED,
      source: SessionNotificationSource.ACTIVITY_STATUS,
    };
    const activity = {
      kind: SessionNotificationKind.ACTIVITY,
      source: SessionNotificationSource.AGENT_TOOL_CALL,
    };

    // When / Then
    expect([dotForNotification(attention), dotForNotification(activity)]).toEqual([
      "attention",
      "activity",
    ]);
  });

  it("a kind this build does not know lights nothing", () => {
    // Given an event of no known kind
    const unknown = {
      kind: SessionNotificationKind.UNSPECIFIED,
      source: SessionNotificationSource.ACTIVITY_STATUS,
    };

    // When / Then
    expect(dotForNotification(unknown)).toBe(null);
  });
});
