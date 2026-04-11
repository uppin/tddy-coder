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

function paramsForSession(prefix: string): LiveKitConnectionParams {
  return {
    livekitUrl: "ws://127.0.0.1:7880",
    roomName: `${prefix}-room`,
    identity: `${prefix}-identity`,
    serverIdentity: `${prefix}-server`,
    debugLogging: false,
  };
}

describe("acceptance: multiSessionState — add second session without removing first", () => {
  it("after connect id1 then connect id2, state contains both keys", () => {
    const id1 = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeee1111";
    const id2 = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeee2222";
    let map: SessionAttachmentMap = new Map();
    map = addSessionAttachment(map, id1, paramsForSession("a"));
    map = addSessionAttachment(map, id2, paramsForSession("b"));
    expect(map.size).toBe(2);
    expect(map.has(id1)).toBe(true);
    expect(map.has(id2)).toBe(true);
  });

  it("id1 LiveKit params are unchanged when id2 is added", () => {
    const id1 = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeee1111";
    const id2 = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeee2222";
    const p1 = paramsForSession("first");
    let map: SessionAttachmentMap = new Map();
    map = addSessionAttachment(map, id1, p1);
    map = addSessionAttachment(map, id2, paramsForSession("second"));
    expect(map.get(id1)?.roomName).toBe(p1.roomName);
    expect(map.get(id1)?.identity).toBe(p1.identity);
    expect(map.get(id2)?.roomName).toBe("second-room");
  });
});

describe("multiSessionState — removeSessionAttachment (per-session teardown)", () => {
  it("removes only the requested sessionId and preserves other attachments", () => {
    const id1 = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeee1111";
    const id2 = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeee2222";
    const map: SessionAttachmentMap = new Map([
      [id1, paramsForSession("a")],
      [id2, paramsForSession("b")],
    ]);
    const next = removeSessionAttachment(map, id1);
    expect(next.has(id2)).toBe(true);
    expect(next.has(id1)).toBe(false);
  });
});

describe("multiSessionState — connectionAttachedTerminalTestId (per-session attachment roots)", () => {
  it("returns stable per-session data-testid for Cypress attachment roots", () => {
    const sid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeee1111";
    expect(connectionAttachedTerminalTestId(sid)).toBe(`connection-attached-terminal-${sid}`);
  });
});

describe("acceptance: multiSessionState — focused session from pathname with multiple entries", () => {
  it("pathname /terminal/idB selects idB when both idA and idB are attached", () => {
    const idA = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeaaaa";
    const idB = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeebbbb";
    const map: SessionAttachmentMap = new Map([
      [idA, paramsForSession("a")],
      [idB, paramsForSession("b")],
    ]);
    const path = terminalPathForSessionId(idB);
    expect(focusedSessionIdFromPathname(path, map)).toBe(idB);
  });
});
