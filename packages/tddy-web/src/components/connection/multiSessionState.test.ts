import { describe, expect, it } from "bun:test";
import {
  addSessionAttachment,
  connectionAttachedTerminalTestId,
  focusedSessionIdFromPathname,
  removeSessionAttachment,
  type LiveKitConnectionParams,
  type SessionAttachmentMap,
} from "./multiSessionState";
import { terminalPathForSessionId } from "../../routing/appRoutes";

function aConnectionParams(prefix: string): LiveKitConnectionParams {
  return {
    livekitUrl: "ws://127.0.0.1:7880",
    roomName: `${prefix}-room`,
    identity: `${prefix}-identity`,
    serverIdentity: `${prefix}-server`,
    debugLogging: false,
  };
}

describe("multiSessionState — addSessionAttachment", () => {
  it("retains both sessions when a second is added to an existing attachment map", () => {
    // Given
    const id1 = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeee1111";
    const id2 = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeee2222";
    let map: SessionAttachmentMap = new Map();

    // When
    map = addSessionAttachment(map, id1, aConnectionParams("a"));
    map = addSessionAttachment(map, id2, aConnectionParams("b"));

    // Then
    expect(map.size).toBe(2);
    expect(map.has(id1)).toBe(true);
    expect(map.has(id2)).toBe(true);
  });

  it("does not overwrite the first session's LiveKit params when a second is added", () => {
    // Given
    const id1 = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeee1111";
    const id2 = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeee2222";
    const p1 = aConnectionParams("first");
    let map: SessionAttachmentMap = new Map();

    // When
    map = addSessionAttachment(map, id1, p1);
    map = addSessionAttachment(map, id2, aConnectionParams("second"));

    // Then
    expect(map.get(id1)?.roomName).toBe(p1.roomName);
    expect(map.get(id1)?.identity).toBe(p1.identity);
    expect(map.get(id2)?.roomName).toBe("second-room");
  });
});

describe("multiSessionState — removeSessionAttachment", () => {
  it("removes only the requested session and preserves other attachments", () => {
    // Given
    const id1 = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeee1111";
    const id2 = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeee2222";
    const map: SessionAttachmentMap = new Map([
      [id1, aConnectionParams("a")],
      [id2, aConnectionParams("b")],
    ]);

    // When
    const next = removeSessionAttachment(map, id1);

    // Then
    expect(next.has(id2)).toBe(true);
    expect(next.has(id1)).toBe(false);
  });
});

describe("multiSessionState — connectionAttachedTerminalTestId", () => {
  it("returns a stable per-session data-testid for Cypress attachment roots", () => {
    // Given
    const sid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeee1111";

    // When
    const testId = connectionAttachedTerminalTestId(sid);

    // Then
    expect(testId).toBe(`connection-attached-terminal-${sid}`);
  });
});

describe("multiSessionState — focusedSessionIdFromPathname", () => {
  it("selects the session whose id matches the terminal pathname when multiple sessions are attached", () => {
    // Given
    const idA = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeaaaa";
    const idB = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeebbbb";
    const map: SessionAttachmentMap = new Map([
      [idA, aConnectionParams("a")],
      [idB, aConnectionParams("b")],
    ]);
    const path = terminalPathForSessionId(idB);

    // When
    const focused = focusedSessionIdFromPathname(path, map);

    // Then
    expect(focused).toBe(idB);
  });
});
