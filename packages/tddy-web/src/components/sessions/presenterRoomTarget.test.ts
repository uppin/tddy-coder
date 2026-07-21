import { describe, it, expect } from "bun:test";
import { presenterRoomTargetFor } from "./presenterRoomTarget";
import type { SessionAttachmentState } from "./useSessionAttachment";

// Tests for the pure `presenterRoomTargetFor` derivation — the PR-Stack Chat Screen's own
// dedicated LiveKit room connection is derived from the session's `connectSession`/
// `resumeSession` attachment (the same room/url the terminal independently connects to via
// `SessionLiveKitTerminal`), never from `SessionMainPane`'s VNC-purpose `room` prop.

describe("presenterRoomTargetFor", () => {
  describe("attachment not yet connected over LiveKit", () => {
    it("returns null while the attachment is idle", () => {
      // Given
      const attachment: SessionAttachmentState = { status: "idle" };

      // When
      const target = presenterRoomTargetFor(attachment);

      // Then
      expect(target).toBeNull();
    });

    it("returns null while the attachment is still connecting", () => {
      // Given
      const attachment: SessionAttachmentState = { status: "connecting" };

      // When
      const target = presenterRoomTargetFor(attachment);

      // Then
      expect(target).toBeNull();
    });

    it("returns null for a plain gRPC attachment (no LiveKit room at all)", () => {
      // Given
      const attachment: SessionAttachmentState = {
        status: "connected-grpc",
        sessionId: "grpc-only-session-0001",
      };

      // When
      const target = presenterRoomTargetFor(attachment);

      // Then
      expect(target).toBeNull();
    });

    it("returns null when the attachment failed", () => {
      // Given
      const attachment: SessionAttachmentState = {
        status: "error",
        error: "daemon unreachable",
      };

      // When
      const target = presenterRoomTargetFor(attachment);

      // Then
      expect(target).toBeNull();
    });
  });

  describe("attachment connected over LiveKit", () => {
    const CONNECTED_ATTACHMENT: SessionAttachmentState = {
      status: "connected-livekit",
      sessionId: "pr-stack-presenter-room-0001",
      livekitRoom: "daemon-pr-stack-presenter-room-0001",
      livekitUrl: "wss://livekit.internal:7880",
      livekitServerIdentity: "daemon-pr-stack-presenter-room-0001",
      identity: "browser-pr-stack-presenter-room-0001-1719999999999",
    };

    it("targets the session's own attached room and url — not SessionMainPane's VNC room", () => {
      // Given / When
      const target = presenterRoomTargetFor(CONNECTED_ATTACHMENT);

      // Then
      expect(target?.roomName).toBe("daemon-pr-stack-presenter-room-0001");
      expect(target?.url).toBe("wss://livekit.internal:7880");
    });

    it("uses the injected identity generator rather than the terminal's own browser identity", () => {
      // Given — the terminal already has its own `attachment.identity`; the presenter
      // connection must be a distinct participant, not a duplicate join under the same identity
      const makeIdentity = () => "browser-presenter-pr-stack-presenter-room-0001-fixed";

      // When
      const target = presenterRoomTargetFor(CONNECTED_ATTACHMENT, makeIdentity);

      // Then
      expect(target?.identity).toBe("browser-presenter-pr-stack-presenter-room-0001-fixed");
      expect(target?.identity).not.toBe(CONNECTED_ATTACHMENT.identity);
    });
  });
});
