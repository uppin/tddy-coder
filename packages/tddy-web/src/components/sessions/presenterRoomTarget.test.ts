import { describe, it, expect } from "bun:test";
import { presenterRoomTargetFor } from "./presenterRoomTarget";
import type { SessionAttachmentHint } from "../../rpc/connections/session";

// Tests for the pure `presenterRoomTargetFor` derivation — the PR-Stack Chat Screen's own
// dedicated LiveKit room connection is derived from the attached session's routing hint (the same
// room/url the terminal independently connects to via `SessionLiveKitTerminal`), never from
// `SessionMainPane`'s VNC-purpose `room` prop.

describe("presenterRoomTargetFor", () => {
  describe("no room to join", () => {
    it("returns null while no session is attached", () => {
      // Given / When
      const target = presenterRoomTargetFor(null);

      // Then
      expect(target).toBeNull();
    });

    it("returns null for a session its host serves itself (the hint names no room)", () => {
      // Given
      const hint: SessionAttachmentHint = { sessionId: "host-served-session-0001" };

      // When
      const target = presenterRoomTargetFor(hint);

      // Then
      expect(target).toBeNull();
    });
  });

  describe("a session carried over its own room", () => {
    const A_ROOM_BACKED_HINT: SessionAttachmentHint = {
      sessionId: "pr-stack-presenter-room-0001",
      room: "daemon-pr-stack-presenter-room-0001",
      url: "wss://livekit.internal:7880",
      serverIdentity: "daemon-pr-stack-presenter-room-0001",
    };

    it("targets the session's own attached room and url — not SessionMainPane's VNC room", () => {
      // Given / When
      const target = presenterRoomTargetFor(A_ROOM_BACKED_HINT);

      // Then
      expect(target?.roomName).toBe("daemon-pr-stack-presenter-room-0001");
      expect(target?.url).toBe("wss://livekit.internal:7880");
    });

    it("uses the injected identity generator rather than the session's own participant identity", () => {
      // Given — the session's process is already on that room as `serverIdentity`; the presenter
      // connection must be a distinct participant, not a duplicate join under the same identity
      const makeIdentity = () => "browser-presenter-pr-stack-presenter-room-0001-fixed";

      // When
      const target = presenterRoomTargetFor(A_ROOM_BACKED_HINT, makeIdentity);

      // Then
      expect(target?.identity).toBe("browser-presenter-pr-stack-presenter-room-0001-fixed");
      expect(target?.identity).not.toBe(A_ROOM_BACKED_HINT.serverIdentity);
    });
  });
});
