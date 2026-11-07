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
    openSession: () => {
      throw new Error("this connection is a routing stand-in and opens no sessions");
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

  it("replaces a provider registered again under the same id, in the place it already held", () => {
    // Given a deliberate precedence — the desktop's own wire ahead of LiveKit — and then a LiveKit
    // provider that registers again claiming a different host, which is what a reconnect does
    const registry = new ConnectionProviderRegistry();
    registry.register(aProviderNamed("ipc", ["this-host"]));
    registry.register(aProviderNamed("livekit", ["this-host", "an-old-peer"]));
    registry.register(aProviderNamed("livekit", ["this-host", "a-new-peer"]));

    // When the hosts are resolved
    const stale = registry.connectHost("an-old-peer");
    const current = registry.connectHost("a-new-peer");
    const contested = registry.connectHost("this-host");

    // Then only the latest registration answers — two providers under one id would make precedence
    // depend on which copy was asked
    expect(stale).toBeNull();
    expect(current?.providerId).toEqual("livekit");

    // And the re-registration kept its place rather than moving to the back of the queue: a
    // reconnecting wire must not overtake, or fall behind, the wires it was ordered against, or a
    // dropped common room would silently start routing the desktop's own host through LiveKit
    expect(registry.providerIds()).toEqual(["ipc", "livekit"]);
    expect(contested?.providerId).toEqual("ipc");
  });
});

/**
 * The observable half of the registry.
 *
 * A wire registers itself while the component that owns it renders — the only moment early enough
 * for the subtree's first paint to resolve its hosts — and a render may not update other components.
 * So the notification is deferred to a microtask and coalesced, and `revision()` is the
 * `useSyncExternalStore` snapshot every consumer compares. Getting either wrong is invisible in the
 * routing tests above and very visible in the app: a missed bump leaves a screen holding the `null`
 * connection it resolved before the room arrived, and a spurious one rebuilds every client.
 */
describe("ConnectionProviderRegistry observability", () => {
  it("bumps its revision when a provider registers", () => {
    // Given a registry nothing has been offered to yet
    const registry = new ConnectionProviderRegistry();
    const before = registry.revision();

    // When a wire comes up
    registry.register(aProviderNamed("livekit", ["a-peer"]));

    // Then the snapshot every consumer compares has moved. This is the whole mechanism by which a
    // screen that resolved "unreachable" before the room existed asks again once it does.
    expect(registry.revision()).toBeGreaterThan(before);
  });

  it("treats re-registering the very same instance as no change at all", () => {
    // Given a provider that is already registered
    const registry = new ConnectionProviderRegistry();
    const provider = aProviderNamed("livekit", ["a-peer"]);
    registry.register(provider);
    const settled = registry.revision();

    // When the same instance is offered again — what `LiveKitConnections` does on every render,
    // including the renders React discards
    registry.register(provider);
    registry.register(provider);

    // Then nothing moved. A bump here would invalidate every `useHostConnection` and every cached
    // client in the app on a render that changed no routing whatsoever.
    expect(registry.revision()).toEqual(settled);
  });

  it("bumps its revision when a different instance replaces one under the same id", () => {
    // Given a registered wire
    const registry = new ConnectionProviderRegistry();
    registry.register(aProviderNamed("livekit", ["an-old-peer"]));
    const before = registry.revision();

    // When a *different* provider takes over that id — a new room, so every transport built against
    // the old one is dead
    registry.register(aProviderNamed("livekit", ["a-new-peer"]));

    // Then consumers are told, because this is the one case where they must re-resolve
    expect(registry.revision()).toBeGreaterThan(before);
  });

  it("notifies subscribers once for several registrations in the same task", async () => {
    // Given a subscriber
    const registry = new ConnectionProviderRegistry();
    let notifications = 0;
    registry.subscribe(() => {
      notifications += 1;
    });

    // When two wires register back to back, as an app root registering IPC and then LiveKit does
    registry.register(aProviderNamed("ipc", ["this-host"]));
    registry.register(aProviderNamed("livekit", ["a-peer"]));

    // Then nothing has been delivered synchronously — a registration happens during someone's
    // render, and notifying from there would update other components mid-render
    expect(notifications).toEqual(0);

    // And when the task drains, one notification arrives for both, carrying the settled revision
    await Promise.resolve();
    expect(notifications).toEqual(1);
    expect(registry.revision()).toEqual(2);
  });

  it("notifies again for a registration in a later task", async () => {
    // Given a subscriber that has already seen one coalesced notification
    const registry = new ConnectionProviderRegistry();
    let notifications = 0;
    registry.subscribe(() => {
      notifications += 1;
    });
    registry.register(aProviderNamed("ipc", ["this-host"]));
    await Promise.resolve();

    // When a second wire comes up later — the common room finishing its join long after the app
    // rendered
    registry.register(aProviderNamed("livekit", ["a-peer"]));
    await Promise.resolve();

    // Then it is delivered too: coalescing is per task, not once per registry
    expect(notifications).toEqual(2);
  });

  it("stops delivering to a subscriber that unsubscribed", async () => {
    // Given a subscriber that has gone away — a consumer unmounting
    const registry = new ConnectionProviderRegistry();
    let notifications = 0;
    const unsubscribe = registry.subscribe(() => {
      notifications += 1;
    });
    unsubscribe();

    // When a wire registers
    registry.register(aProviderNamed("livekit", ["a-peer"]));
    await Promise.resolve();

    // Then nothing is delivered, and the revision still moved for whoever reads it next
    expect(notifications).toEqual(0);
    expect(registry.revision()).toEqual(1);
  });
});
