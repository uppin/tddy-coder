/**
 * Unit tests for reading a daemon's attach reply into a transport-neutral hint.
 *
 * This is the branch that produced the whole two-status problem: today
 * `attachmentStateFromResponse` decides `connected-livekit` vs `connected-grpc` from
 * `resp.livekitRoom !== ""`, and every consumer downstream then re-derives what it may do from which
 * of the two it got. These tests pin the single reading that replaces it.
 *
 * Technical: `packages/tddy-web/docs/session-connections.md`
 */

import { describe, it, expect } from "bun:test";
import { attachmentHintFromReply, capabilitiesForHint, type AttachReply } from "./sessionAttachment";

const A_SESSION = "session-0001";

/** What the daemon replies for a session it publishes into a LiveKit room. */
function aRoomBackedReply(): AttachReply {
  return {
    livekitRoom: "daemon-session-0001",
    livekitUrl: "wss://livekit.example",
    livekitServerIdentity: "daemon-instance-a-session-0001",
  };
}

/** What it replies for a session it serves itself — the shape a desktop app over IPC produces. */
function aHostServedReply(): AttachReply {
  return { livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" };
}

describe("reading an attach reply", () => {
  it("carries the room, its url and the session's own server identity when the reply names one", () => {
    // When a room-backed session is attached
    const hint = attachmentHintFromReply(A_SESSION, aRoomBackedReply());

    // Then everything the provider needs to reach the session process is on the hint, and nothing
    // else in the app has to read a LiveKit field again
    expect(hint).toEqual({
      sessionId: A_SESSION,
      room: "daemon-session-0001",
      url: "wss://livekit.example",
      serverIdentity: "daemon-instance-a-session-0001",
    });
  });

  it("produces a hint with no room when the daemon serves the session itself", () => {
    // When a session on a host that answers its own session RPC is attached
    const hint = attachmentHintFromReply(A_SESSION, aHostServedReply());

    // Then the hint names only the session. Crucially the blank fields are *absent*, not empty
    // strings: an empty string that survives is exactly what let `SessionsDrawerScreen` fabricate
    // a state carrying four blank LiveKit fields to satisfy the old union.
    expect(hint).toEqual({ sessionId: A_SESSION });
  });

  it("drops a blank url or server identity even when a room is named", () => {
    // Given a partially-filled reply — an older daemon, or a room with no separate session process
    const hint = attachmentHintFromReply(A_SESSION, {
      livekitRoom: "daemon-session-0001",
      livekitUrl: "",
      livekitServerIdentity: "",
    });

    // Then the absent parts are absent, so a provider can tell "not told" from "told nothing"
    expect(hint.room).toEqual("daemon-session-0001");
    expect(hint.url).toBeUndefined();
    expect(hint.serverIdentity).toBeUndefined();
  });

  it("gives a room-backed session media and presence as well as rpc", () => {
    // Given a session carried over LiveKit
    const capabilities = capabilitiesForHint(attachmentHintFromReply(A_SESSION, aRoomBackedReply()));

    // Then its VNC, screen-sharing and participant surfaces apply — which is what node 4 gates on
    expect([...capabilities].sort()).toEqual(["media", "presence", "rpc"]);
  });

  it("gives a host-served session rpc only", () => {
    // Given a session the host answers itself
    const capabilities = capabilitiesForHint(attachmentHintFromReply(A_SESSION, aHostServedReply()));

    // Then it can make calls and nothing else. A frame pipe cannot carry a video track, so the
    // media surfaces do not apply to it — they are absent, not broken.
    expect([...capabilities]).toEqual(["rpc"]);
  });
});
