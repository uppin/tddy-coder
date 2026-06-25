/**
 * Tests for `anInMemoryRpcBackend` from `tddy-connectrpc-testkit`.
 *
 * These run with bun:test and use the generated service descriptors from
 * `packages/tddy-web/src/gen/` as realistic test subjects.
 *
 * Structure:
 *   Given  — build a backend with stubs / implementations
 *   When   — call through a `createClient` built from `backend.transport()`
 *   Then   — assert the response and recorded calls
 */

import { describe, expect, it } from "bun:test";
import { ConnectError, createClient, Code } from "@connectrpc/connect";

import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";

import { AuthService } from "../gen/auth_pb";
import { TokenService } from "../gen/token_pb";
import { ConnectionService } from "../gen/connection_pb";

// ---------------------------------------------------------------------------
// .onUnary — single-method stub
// ---------------------------------------------------------------------------

describe("anInMemoryRpcBackend — .onUnary", () => {
  it("returns the stubbed response for a registered method", async () => {
    // Given
    const backend = anInMemoryRpcBackend().onUnary(
      AuthService.method.getAuthStatus,
      () => ({ authenticated: false }),
    );
    const client = createClient(AuthService, backend.transport());

    // When
    const response = await client.getAuthStatus({ sessionToken: "tok" });

    // Then — proto3 default for bool is false; confirmed via explicit stub
    expect(response.authenticated).toBe(false);
  });

  it("receives the full request message in the handler", async () => {
    // Given
    const backend = anInMemoryRpcBackend().onUnary(
      TokenService.method.generateToken,
      (req) => ({ token: `token-for-${req.room}`, ttlSeconds: 3600n }),
    );
    const client = createClient(TokenService, backend.transport());

    // When
    const response = await client.generateToken({ room: "my-room", identity: "alice" });

    // Then
    expect(response.token).toBe("token-for-my-room");
    expect(response.ttlSeconds).toBe(3600n);
  });

  it("responds with Code.Unimplemented for methods not registered", async () => {
    // Given
    const backend = anInMemoryRpcBackend();
    const client = createClient(AuthService, backend.transport());

    // When
    const err = await client.getAuthStatus({ sessionToken: "" }).catch((e: unknown) => e);

    // Then
    expect(err).toBeInstanceOf(ConnectError);
    expect((err as ConnectError).code).toBe(Code.Unimplemented);
  });
});

// ---------------------------------------------------------------------------
// .implement — full service implementation
// ---------------------------------------------------------------------------

describe("anInMemoryRpcBackend — .implement", () => {
  it("routes calls to the registered service implementation", async () => {
    // Given
    const backend = anInMemoryRpcBackend().implement(AuthService, {
      getAuthStatus: () => ({ authenticated: true }),
      getAuthUrl: () => ({ authUrl: "https://github.com/login/oauth/authorize" }),
    });
    const client = createClient(AuthService, backend.transport());

    // When
    const response = await client.getAuthStatus({ sessionToken: "" });

    // Then
    expect(response.authenticated).toBe(true);
  });

  it("allows combining .implement and .onUnary on the same backend", async () => {
    // Given — .implement covers AuthService, .onUnary handles one TokenService method
    const backend = anInMemoryRpcBackend()
      .implement(AuthService, {
        getAuthStatus: () => ({ authenticated: true }),
      })
      .onUnary(TokenService.method.generateToken, () => ({ token: "tok", ttlSeconds: 60n }));

    const authClient = createClient(AuthService, backend.transport());
    const tokenClient = createClient(TokenService, backend.transport());

    // When
    const authResp = await authClient.getAuthStatus({ sessionToken: "" });
    const tokenResp = await tokenClient.generateToken({ room: "r", identity: "i" });

    // Then
    expect(authResp.authenticated).toBe(true);
    expect(tokenResp.token).toBe("tok");
  });
});

// ---------------------------------------------------------------------------
// .failWith — error stub
// ---------------------------------------------------------------------------

describe("anInMemoryRpcBackend — .failWith", () => {
  it("rejects with the given Connect error code", async () => {
    // Given
    const backend = anInMemoryRpcBackend().failWith(
      AuthService.method.getAuthStatus,
      Code.Unauthenticated,
      "no session",
    );
    const client = createClient(AuthService, backend.transport());

    // When
    const err = await client.getAuthStatus({ sessionToken: "stale" }).catch((e: unknown) => e);

    // Then
    expect(err).toBeInstanceOf(ConnectError);
    expect((err as ConnectError).code).toBe(Code.Unauthenticated);
    expect((err as ConnectError).rawMessage).toBe("no session");
  });

  it("rejects ConnectionService calls with PermissionDenied when stubbed", async () => {
    // Given
    const backend = anInMemoryRpcBackend().failWith(
      ConnectionService.method.deleteSession,
      Code.PermissionDenied,
    );
    const client = createClient(ConnectionService, backend.transport());

    // When
    const err = await client
      .deleteSession({ sessionToken: "tok", sessionId: "sess-1" })
      .catch((e: unknown) => e);

    // Then
    expect(err).toBeInstanceOf(ConnectError);
    expect((err as ConnectError).code).toBe(Code.PermissionDenied);
  });
});

// ---------------------------------------------------------------------------
// .callsTo — call recording
// ---------------------------------------------------------------------------

describe("anInMemoryRpcBackend — .callsTo", () => {
  it("records the request message for each unary call", async () => {
    // Given
    const backend = anInMemoryRpcBackend().onUnary(
      TokenService.method.generateToken,
      () => ({ token: "tok", ttlSeconds: 10n }),
    );
    const client = createClient(TokenService, backend.transport());

    // When
    await client.generateToken({ room: "room-a", identity: "alice" });
    await client.generateToken({ room: "room-b", identity: "bob" });

    // Then — both calls recorded, in order
    const calls = backend.callsTo(TokenService.method.generateToken);
    expect(calls).toHaveLength(2);
    expect(calls[0].room).toBe("room-a");
    expect(calls[0].identity).toBe("alice");
    expect(calls[1].room).toBe("room-b");
    expect(calls[1].identity).toBe("bob");
  });

  it("returns an empty array when a method has not been called", () => {
    // Given
    const backend = anInMemoryRpcBackend();

    // Then
    expect(backend.callsTo(TokenService.method.generateToken)).toEqual([]);
  });

  it("resets recorded calls on .resetCalls()", async () => {
    // Given
    const backend = anInMemoryRpcBackend().onUnary(
      TokenService.method.generateToken,
      () => ({ token: "tok", ttlSeconds: 0n }),
    );
    const client = createClient(TokenService, backend.transport());
    await client.generateToken({ room: "r", identity: "i" });

    // When
    backend.resetCalls();

    // Then
    expect(backend.callsTo(TokenService.method.generateToken)).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// .transport() — caching
// ---------------------------------------------------------------------------

describe("anInMemoryRpcBackend — .transport()", () => {
  it("returns the same Transport instance on repeated calls", () => {
    // Given
    const backend = anInMemoryRpcBackend();

    // When
    const t1 = backend.transport();
    const t2 = backend.transport();

    // Then
    expect(t1).toBe(t2);
  });
});
