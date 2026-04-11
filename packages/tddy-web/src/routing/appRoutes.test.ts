import { describe, expect, it } from "bun:test";
import {
  PROJECT_ROW_ROUTE_PREFIX,
  TERMINAL_SESSION_ROUTE_PREFIX,
  isAuthCallbackPath,
  isSessionListPath,
  parseProjectRowKeyFromPathname,
  parseTerminalSessionIdFromPathname,
  projectPathForRowKey,
  terminalDeepLinkSessionPath,
  terminalPathForSessionId,
} from "./appRoutes";
import { parseProjectRowKeyForConnectionScreen } from "./projectRoute";

describe("appRoutes helpers (canonical path rules)", () => {
  it("terminalPathForSessionId maps a session id to /terminal/:id", () => {
    expect(terminalPathForSessionId("sess-a")).toBe(`${TERMINAL_SESSION_ROUTE_PREFIX}/sess-a`);
  });

  it("parseTerminalSessionIdFromPathname extracts id from /terminal/:sessionId", () => {
    expect(parseTerminalSessionIdFromPathname(`${TERMINAL_SESSION_ROUTE_PREFIX}/abc-123`)).toBe("abc-123");
  });

  it("isSessionListPath is true for home / session list", () => {
    expect(isSessionListPath("/")).toBe(true);
  });

  it("auth_callback_route_unchanged_and_still_handles_oauth", () => {
    expect(isAuthCallbackPath("/auth/callback")).toBe(true);
  });

  it("acceptance: terminalDeepLinkSessionPath stays aligned with terminalPathForSessionId (encoded ids)", () => {
    const id = "sess/with space";
    expect(terminalDeepLinkSessionPath(id)).toBe(terminalPathForSessionId(id));
  });
});

describe("appRoutes project row URLs (acceptance — project sessions screen)", () => {
  it("acceptance: HomeOverflowNavigatesToProjectScreenWithCorrectEncodedKey — projectPathForRowKey encodes row key in one path segment", () => {
    const rowKey = "proj-1__workstation-1";
    expect(projectPathForRowKey(rowKey)).toBe(
      `${PROJECT_ROW_ROUTE_PREFIX}/${encodeURIComponent(rowKey)}`,
    );
  });

  it("acceptance: ProjectScreenListsAllSessionsForProject — parseProjectRowKeyFromPathname decodes a single /project/:key segment", () => {
    const rowKey = "cccccccc-dddd-4eee-8fff-999999999999__server-2";
    const path = `${PROJECT_ROW_ROUTE_PREFIX}/${encodeURIComponent(rowKey)}`;
    expect(parseProjectRowKeyFromPathname(path)).toBe(rowKey);
    expect(parseProjectRowKeyFromPathname(`${TERMINAL_SESSION_ROUTE_PREFIX}/abc-123`)).toBeNull();
    expect(parseProjectRowKeyFromPathname(`${PROJECT_ROW_ROUTE_PREFIX}/a/b`)).toBeNull();
    expect(parseProjectRowKeyFromPathname(PROJECT_ROW_ROUTE_PREFIX)).toBeNull();
  });
});

describe("parseProjectRowKeyForConnectionScreen (granular — RED wiring + marker)", () => {
  it("granular: wrapper returns decoded row key same as parseProjectRowKeyFromPathname", () => {
    const rowKey = "wrapper-delegate-key";
    const path = `${PROJECT_ROW_ROUTE_PREFIX}/${encodeURIComponent(rowKey)}`;
    expect(parseProjectRowKeyForConnectionScreen(path)).toBe(rowKey);
  });
});
