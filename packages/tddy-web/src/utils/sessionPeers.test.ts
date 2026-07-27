import { describe, expect, it } from "bun:test";
import { aSessionEntry } from "../test-utils";
import { sessionPeers } from "./sessionPeers";

/**
 * Acceptance tests for `sessionPeers` — the pure util that derives the peer agent sessions of a
 * given session. A peer is a session whose `orchestratorSessionId` equals the given session's id.
 */

const CURRENT_SESSION_ID = "session-current-aaaa-0000-0000-000000000001";

function aPeerOf(sessionId: string, orchestratorSessionId: string) {
  return aSessionEntry({ sessionId, orchestratorSessionId });
}

describe("sessionPeers", () => {
  it("returns an empty array when the session list is empty", () => {
    // Given
    const sessions: ReturnType<typeof aSessionEntry>[] = [];

    // When
    const peers = sessionPeers(sessions, CURRENT_SESSION_ID);

    // Then
    expect(peers).toEqual([]);
  });

  it("returns only the sessions whose orchestratorSessionId matches the current session", () => {
    // Given — two peers of the current session, plus one peer of another session and one standalone
    const peer1 = aPeerOf("peer-1", CURRENT_SESSION_ID);
    const peer2 = aPeerOf("peer-2", CURRENT_SESSION_ID);
    const otherChild = aPeerOf("other-child", "some-other-orchestrator");
    const standalone = aSessionEntry({ sessionId: "standalone", orchestratorSessionId: "" });
    const sessions = [peer1, peer2, otherChild, standalone];

    // When
    const peers = sessionPeers(sessions, CURRENT_SESSION_ID);

    // Then
    expect(peers.map((p) => p.sessionId)).toEqual(["peer-1", "peer-2"]);
  });

  it("excludes sessions whose orchestratorSessionId points to a different session", () => {
    // Given
    const otherChild = aPeerOf("other-child", "some-other-orchestrator");
    const sessions = [otherChild];

    // When
    const peers = sessionPeers(sessions, CURRENT_SESSION_ID);

    // Then
    expect(peers).toEqual([]);
  });

  it("excludes sessions with an empty orchestratorSessionId", () => {
    // Given
    const standalone = aSessionEntry({ sessionId: "standalone", orchestratorSessionId: "" });
    const sessions = [standalone];

    // When
    const peers = sessionPeers(sessions, CURRENT_SESSION_ID);

    // Then
    expect(peers).toEqual([]);
  });

  it("does not include the current session itself even if it self-references", () => {
    // Given — a malformed self-referencing entry plus a real peer
    const selfRef = aPeerOf(CURRENT_SESSION_ID, CURRENT_SESSION_ID);
    const realPeer = aPeerOf("real-peer", CURRENT_SESSION_ID);
    const sessions = [selfRef, realPeer];

    // When
    const peers = sessionPeers(sessions, CURRENT_SESSION_ID);

    // Then — only the real peer is returned, not the self-reference
    expect(peers.map((p) => p.sessionId)).toEqual(["real-peer"]);
  });
});
