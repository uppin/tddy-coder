import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { TokenService } from "../gen/token_pb";
import { AuthService } from "../gen/auth_pb";
import { aClientsPortDouble } from "../testkit";
import {
  ClientsProvider,
  type ClientsPort,
  useDaemonClient,
  useHostClient,
  useHostLabel,
  useHttpClient,
  useSelectedDaemon,
} from "./clients";

afterEach(cleanup);

function inPort(port: ClientsPort) {
  return ({ children }: { children: ReactNode }) => <ClientsProvider port={port}>{children}</ClientsProvider>;
}

function aTokenBackend() {
  return anInMemoryRpcBackend().onUnary(TokenService.method.generateToken, (req) => ({
    token: `token-for-${req.identity}`,
    ttlSeconds: 60n,
  }));
}

describe("the clients port", () => {
  test("useDaemonClient reaches the selected daemon through the port", async () => {
    // Given
    const backend = aTokenBackend();
    const port = aClientsPortDouble(backend, { daemons: [{ instanceId: "host-a", label: "A" }] });

    // When
    const { result } = renderHook(() => useDaemonClient(TokenService), { wrapper: inPort(port) });
    const reply = await result.current!.generateToken({ room: "r", identity: "alice" });

    // Then
    expect(reply.token).toBe("token-for-alice");
    expect(backend.callsTo(TokenService.method.generateToken)).toHaveLength(1);
  });

  test("useHostClient reaches a named host through the registry", async () => {
    // Given
    const backend = aTokenBackend();
    const port = aClientsPortDouble(backend, {
      daemons: [
        { instanceId: "host-a", label: "A" },
        { instanceId: "host-b", label: "B" },
      ],
    });

    // When
    const { result } = renderHook(() => useHostClient(TokenService, "host-b"), { wrapper: inPort(port) });
    const reply = await result.current!.generateToken({ room: "r", identity: "bob" });

    // Then
    expect(reply.token).toBe("token-for-bob");
  });

  test("useHostClient returns null for a host no provider claims", () => {
    // Given
    const port = aClientsPortDouble(aTokenBackend(), { daemons: [{ instanceId: "host-a", label: "A" }] });

    // When
    const { result } = renderHook(() => useHostClient(TokenService, "host-unknown"), { wrapper: inPort(port) });

    // Then
    expect(result.current).toBeNull();
  });

  test("useHttpClient uses the port's HTTP transport", async () => {
    // Given
    const backend = anInMemoryRpcBackend().onUnary(AuthService.method.getAuthStatus, () => ({
      isAuthenticated: false,
      sessionToken: "",
    }));
    const port = aClientsPortDouble(backend);

    // When
    const { result } = renderHook(() => useHttpClient(AuthService), { wrapper: inPort(port) });
    const status = await result.current.getAuthStatus({});

    // Then
    expect(status.isAuthenticated).toBe(false);
    expect(backend.callsTo(AuthService.method.getAuthStatus)).toHaveLength(1);
  });

  test("a client is the same instance while its connection holds", () => {
    // Given
    const port = aClientsPortDouble(aTokenBackend());

    // When
    const { result, rerender } = renderHook(() => useDaemonClient(TokenService), { wrapper: inPort(port) });
    const first = result.current;
    rerender();

    // Then
    expect(result.current).toBe(first);
  });

  test("useHostLabel names a listed host by its label", () => {
    // Given
    const port = aClientsPortDouble(aTokenBackend(), { daemons: [{ instanceId: "host-a", label: "Laptop" }] });

    // When
    const { result } = renderHook(() => useHostLabel(), { wrapper: inPort(port) });

    // Then
    expect(result.current("host-a")).toContain("Laptop");
  });

  test("useSelectedDaemon outside a provider throws naming the port", () => {
    // When / Then
    expect(() => renderHook(() => useSelectedDaemon())).toThrow(/ClientsProvider/);
  });
});
