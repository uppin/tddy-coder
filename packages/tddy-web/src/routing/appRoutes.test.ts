import { describe, expect, it } from "bun:test";
import {
  TERMINAL_SESSION_ROUTE_PREFIX,
  isAuthCallbackPath,
  isSessionListPath,
  parseTerminalSessionIdFromPathname,
  terminalPathForSessionId,
} from "./appRoutes";

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
});
