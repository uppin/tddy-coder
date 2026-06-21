import { describe, expect, it } from "bun:test";
import {
  TERMINAL_SESSION_ROUTE_PREFIX,
  isAuthCallbackPath,
  isSessionListPath,
  parseTerminalSessionIdFromPathname,
  terminalDeepLinkSessionPath,
  terminalPathForSessionId,
  RPC_PLAYGROUND_ROUTE,
  isRpcPlaygroundPath,
  VMS_ROUTE,
  isVmsPath,
} from "./appRoutes";

describe("appRoutes helpers (canonical path rules)", () => {
  it("builds a /terminal/:id path from a session id", () => {
    // When
    const result = terminalPathForSessionId("sess-a");
    // Then
    expect(result).toBe(`${TERMINAL_SESSION_ROUTE_PREFIX}/sess-a`);
  });

  it("extracts the session id from a /terminal/:sessionId pathname", () => {
    // When
    const result = parseTerminalSessionIdFromPathname(`${TERMINAL_SESSION_ROUTE_PREFIX}/abc-123`);
    // Then
    expect(result).toBe("abc-123");
  });

  it("recognises the home path as the session list", () => {
    // When
    const result = isSessionListPath("/");
    // Then
    expect(result).toBe(true);
  });

  it("recognises /auth/callback as the OAuth callback path", () => {
    // When
    const result = isAuthCallbackPath("/auth/callback");
    // Then
    expect(result).toBe(true);
  });

  it("deep-link session path stays aligned with terminalPathForSessionId for encoded ids", () => {
    // Given
    const id = "sess/with space";

    // When + Then
    expect(terminalDeepLinkSessionPath(id)).toBe(terminalPathForSessionId(id));
  });
});

// These tests fail until RPC_PLAYGROUND_ROUTE and isRpcPlaygroundPath are added to appRoutes.ts.
describe("appRoutes — RPC Playground route helpers", () => {
  it("RPC_PLAYGROUND_ROUTE is /rpc-playground", () => {
    // When + Then
    expect(RPC_PLAYGROUND_ROUTE).toBe("/rpc-playground");
  });

  it("recognises /rpc-playground as the RPC Playground path", () => {
    // When
    const result = isRpcPlaygroundPath("/rpc-playground");
    // Then
    expect(result).toBe(true);
  });

  it("does not match the root path as an RPC Playground path", () => {
    // When
    const result = isRpcPlaygroundPath("/");
    // Then
    expect(result).toBe(false);
  });

  it("does not match /worktrees as an RPC Playground path", () => {
    // When
    const result = isRpcPlaygroundPath("/worktrees");
    // Then
    expect(result).toBe(false);
  });

  it("does not match a terminal session path as an RPC Playground path", () => {
    // When
    const result = isRpcPlaygroundPath(`${TERMINAL_SESSION_ROUTE_PREFIX}/some-session`);
    // Then
    expect(result).toBe(false);
  });

  it("does not match sub-paths under /rpc-playground", () => {
    // When
    const result = isRpcPlaygroundPath("/rpc-playground/extra");
    // Then
    expect(result).toBe(false);
  });
});

describe("appRoutes — VMs route helpers", () => {
  it("VMS_ROUTE is /vms", () => {
    expect(VMS_ROUTE).toBe("/vms");
  });

  it("recognises /vms as the VMs path", () => {
    expect(isVmsPath("/vms")).toBe(true);
  });

  it("does not match root as a VMs path", () => {
    expect(isVmsPath("/")).toBe(false);
  });

  it("does not match /worktrees as a VMs path", () => {
    expect(isVmsPath("/worktrees")).toBe(false);
  });

  it("does not match /rpc-playground as a VMs path", () => {
    expect(isVmsPath("/rpc-playground")).toBe(false);
  });

  it("does not match sub-paths under /vms", () => {
    expect(isVmsPath("/vms/extra")).toBe(false);
  });
});
