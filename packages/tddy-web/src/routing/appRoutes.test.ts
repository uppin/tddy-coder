import { describe, expect, it } from "bun:test";
import {
  TERMINAL_SESSION_ROUTE_PREFIX,
  isAuthCallbackPath,
  parseTerminalSessionIdFromPathname,
  RPC_PLAYGROUND_ROUTE,
  isRpcPlaygroundPath,
  VMS_ROUTE,
  isVmsPath,
  LIVEKIT_ROUTE,
  isLiveKitPath,
  SESSIONS_DRAWER_ROUTE,
  isSessionsDrawerPath,
  sessionsDrawerPathForSession,
  parseSessionsDrawerSessionId,
} from "./appRoutes";

describe("appRoutes helpers (canonical path rules)", () => {
  // The standalone-mode hash-strip effect still parses legacy /terminal/:id links, so this
  // helper is retained even though the dedicated terminal route is removed.
  it("extracts the session id from a /terminal/:sessionId pathname", () => {
    // When
    const result = parseTerminalSessionIdFromPathname(`${TERMINAL_SESSION_ROUTE_PREFIX}/abc-123`);
    // Then
    expect(result).toBe("abc-123");
  });

  it("recognises /auth/callback as the OAuth callback path", () => {
    // When
    const result = isAuthCallbackPath("/auth/callback");
    // Then
    expect(result).toBe(true);
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

describe("appRoutes — LiveKit route helpers", () => {
  it("LIVEKIT_ROUTE is /livekit", () => {
    expect(LIVEKIT_ROUTE).toBe("/livekit");
  });

  it("recognises /livekit as the LiveKit path", () => {
    expect(isLiveKitPath("/livekit")).toBe(true);
  });

  it("does not match root as a LiveKit path", () => {
    expect(isLiveKitPath("/")).toBe(false);
  });

  it("does not match /sessions as a LiveKit path", () => {
    expect(isLiveKitPath("/sessions")).toBe(false);
  });

  it("does not match sub-paths under /livekit", () => {
    expect(isLiveKitPath("/livekit/extra")).toBe(false);
  });
});

describe("appRoutes — sessions drawer route helpers", () => {
  it("SESSIONS_DRAWER_ROUTE is /sessions", () => {
    // When + Then
    expect(SESSIONS_DRAWER_ROUTE).toBe("/sessions");
  });

  it("recognises /sessions as the sessions drawer path", () => {
    // When
    const result = isSessionsDrawerPath("/sessions");
    // Then
    expect(result).toBe(true);
  });

  it("does not match the root path as a sessions drawer path", () => {
    // When
    const result = isSessionsDrawerPath("/");
    // Then
    expect(result).toBe(false);
  });

  it("does not match a terminal session path as a sessions drawer path", () => {
    // When
    const result = isSessionsDrawerPath(`${TERMINAL_SESSION_ROUTE_PREFIX}/some-session`);
    // Then
    expect(result).toBe(false);
  });

  it("does not match /sessions-extra as a sessions drawer path", () => {
    // When
    const result = isSessionsDrawerPath("/sessions-extra");
    // Then
    expect(result).toBe(false);
  });

  it("builds a /sessions/:id deep link for a session id", () => {
    // When
    const path = sessionsDrawerPathForSession("sess-abc-123");
    // Then
    expect(path).toBe("/sessions/sess-abc-123");
  });

  it("URL-encodes the session id in the deep link path", () => {
    // Given
    const id = "session/with space";
    // When
    const path = sessionsDrawerPathForSession(id);
    // Then
    expect(path).toBe(`/sessions/${encodeURIComponent(id)}`);
  });

  it("extracts the session id from a /sessions/:id pathname", () => {
    // When
    const result = parseSessionsDrawerSessionId("/sessions/abc-123");
    // Then
    expect(result).toBe("abc-123");
  });

  it("URL-decodes the session id when extracting from a deep link pathname", () => {
    // Given
    const id = "sess/with space";
    // When
    const result = parseSessionsDrawerSessionId(`/sessions/${encodeURIComponent(id)}`);
    // Then
    expect(result).toBe(id);
  });

  it("returns null for /sessions (no session id segment)", () => {
    // When
    const result = parseSessionsDrawerSessionId("/sessions");
    // Then
    expect(result).toBeNull();
  });

  it("returns null for a non-sessions path", () => {
    // When
    const result = parseSessionsDrawerSessionId("/");
    // Then
    expect(result).toBeNull();
  });

  it("isSessionsDrawerPath matches /sessions/:id deep links", () => {
    // When
    const result = isSessionsDrawerPath("/sessions/some-session-id");
    // Then
    expect(result).toBe(true);
  });
});
