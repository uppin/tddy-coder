/**
 * Unit tests for the connection-provider registry.
 *
 * The registry is what makes LiveKit optional: every daemon-level call site asks it for a host
 * instead of naming a `Room` and a participant identity, so a host build can contribute a provider
 * the browser bundle does not carry. Three rules carry that weight — first match wins, an
 * unreachable host is `null` rather than an error, and an empty registry behaves like "nothing
 * selected yet" rather than like a fault.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-connection-model.md`
 */

import { describe, it, expect } from "bun:test";
import { ConnectionProviderRegistry } from "./registry";
import type { ConnectionCapability, ConnectionProvider, HostConnection } from "./types";

/** A connection that reaches nothing — the tests here assert routing, never traffic. */
function aConnectionTo(
  hostId: string,
  providerId: string,
  capabilities: ConnectionCapability[] = ["rpc"],
): HostConnection {
  return {
    hostId,
    providerId,
    status: "connected",
    error: null,
    capabilities: new Set(capabilities),
    clientFor: () => {
      throw new Error("this connection is a routing stand-in and issues no calls");
    },
    transport: () => {
      throw new Error("this connection is a routing stand-in and issues no calls");
    },
  };
}

/** A provider that claims exactly the hosts it was told to claim, and answers `null` for the rest. */
function aProviderNamed(id: string, claiming: string[]): ConnectionProvider {
  return {
    id,
    connectHost: (hostId) => (claiming.includes(hostId) ? aConnectionTo(hostId, id) : null),
  };
}

describe("ConnectionProviderRegistry", () => {
  it("resolves a host through the first registered provider that claims it", () => {
    // Given the desktop's registration order: its own wire first, LiveKit behind it
    const registry = new ConnectionProviderRegistry();
    registry.register(aProviderNamed("ipc", ["this-host"]));
    registry.register(aProviderNamed("livekit", ["this-host", "a-peer"]));

    // When a host both providers can reach is resolved
    const connection = registry.connectHost("this-host");

    // Then the earlier registration wins — which is how the desktop reaches its own
    // in-process daemon without a round trip through a media server
    expect(connection?.providerId).toEqual("ipc");
  });

  it("falls through to a later provider for a host the earlier one does not claim", () => {
    // Given the same registration order
    const registry = new ConnectionProviderRegistry();
    registry.register(aProviderNamed("ipc", ["this-host"]));
    registry.register(aProviderNamed("livekit", ["this-host", "a-peer"]));

    // When a host only the later provider can reach is resolved
    const connection = registry.connectHost("a-peer");

    // Then it is reached over that provider — a desktop app configured for LiveKit still
    // talks to its peers over LiveKit while keeping its own host on IPC
    expect(connection?.providerId).toEqual("livekit");
  });

  it("answers null for a host no registered provider claims", () => {
    // Given a registry that knows one host
    const registry = new ConnectionProviderRegistry();
    registry.register(aProviderNamed("livekit", ["a-peer"]));

    // When an unknown host is resolved
    const connection = registry.connectHost("a-host-that-left");

    // Then the answer is null, not a throw: a host that walked out of the common room is an
    // ordinary state every call site already renders as "no client yet"
    expect(connection).toBeNull();
  });

  it("answers null for every host when nothing is registered", () => {
    // Given no provider at all — a browser page before the LiveKit provider registers, or a
    // desktop app with no LiveKit configuration
    const registry = new ConnectionProviderRegistry();

    // When any host is resolved
    const connection = registry.connectHost("this-host");

    // Then it is null rather than an error. This is the case that makes LiveKit optional:
    // with nothing registered the app renders its existing "not connected" states instead
    // of failing, so no screen has to know why there is no provider.
    expect(connection).toBeNull();
    expect(registry.providerIds()).toEqual([]);
  });

  it("reports its providers in precedence order", () => {
    // Given two providers registered in a deliberate order
    const registry = new ConnectionProviderRegistry();
    registry.register(aProviderNamed("ipc", ["this-host"]));
    registry.register(aProviderNamed("livekit", ["a-peer"]));

    // When the registry is asked what it holds
    const ids = registry.providerIds();

    // Then the order is the registration order, because that order is the precedence
    expect(ids).toEqual(["ipc", "livekit"]);
  });

  it("replaces a provider registered again under the same id", () => {
    // Given a provider that is registered, then registered again claiming a different host —
    // what a hot reload or a re-registration on reconnect does
    const registry = new ConnectionProviderRegistry();
    registry.register(aProviderNamed("livekit", ["an-old-peer"]));
    registry.register(aProviderNamed("livekit", ["a-new-peer"]));

    // When both hosts are resolved
    const stale = registry.connectHost("an-old-peer");
    const current = registry.connectHost("a-new-peer");

    // Then only the latest registration answers, and the registry holds one entry — two
    // providers under one id would make precedence depend on which copy was asked
    expect(stale).toBeNull();
    expect(current?.providerId).toEqual("livekit");
    expect(registry.providerIds()).toEqual(["livekit"]);
  });
});
