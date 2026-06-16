import { describe, expect, it } from "bun:test";
import {
  TERMINAL_SESSION_ROUTE_PREFIX,
  isAuthCallbackPath,
  isSessionListPath,
  parseTerminalSessionIdFromPathname,
  terminalDeepLinkSessionPath,
  terminalPathForSessionId,
  // These imports fail until RPC_PLAYGROUND_ROUTE and isRpcPlaygroundPath are added to appRoutes.ts.
  RPC_PLAYGROUND_ROUTE,
  isRpcPlaygroundPath,
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

  it("acceptance: terminalDeepLinkSessionPath stays aligned with terminalPathForSessionId (encoded ids)", () => {
    const id = "sess/with space";
    expect(terminalDeepLinkSessionPath(id)).toBe(terminalPathForSessionId(id));
  });
});

// These tests fail until RPC_PLAYGROUND_ROUTE and isRpcPlaygroundPath are added to appRoutes.ts.
describe("appRoutes — RPC Playground route helpers", () => {
  it("RPC_PLAYGROUND_ROUTE is /rpc-playground", () => {
    expect(RPC_PLAYGROUND_ROUTE).toBe("/rpc-playground");
  });

  it("isRpcPlaygroundPath is true for /rpc-playground", () => {
    expect(isRpcPlaygroundPath("/rpc-playground")).toBe(true);
  });

  it("isRpcPlaygroundPath is false for /", () => {
    expect(isRpcPlaygroundPath("/")).toBe(false);
  });

  it("isRpcPlaygroundPath is false for /worktrees", () => {
    expect(isRpcPlaygroundPath("/worktrees")).toBe(false);
  });

  it("isRpcPlaygroundPath is false for a terminal path", () => {
    expect(isRpcPlaygroundPath(`${TERMINAL_SESSION_ROUTE_PREFIX}/some-session`)).toBe(false);
  });

  it("isRpcPlaygroundPath is false for /rpc-playground/extra (no sub-paths)", () => {
    expect(isRpcPlaygroundPath("/rpc-playground/extra")).toBe(false);
  });
});
