/**
 * Unit tests for `SessionRuntimeRegistry` — the per-session runtime store that keeps one
 * mounted terminal per attached session and survives focus switches (explicit-disconnect eviction).
 *
 * Two things are pinned here. The store's own bookkeeping — focus, eviction, byte accounting — and,
 * below it, **who releases a session's connection**: the registry owns every runtime's
 * `SessionConnection`, and nothing else is in a position to close one.
 *
 * Changeset: `2026-07-12-fast-session-change`
 * Technical: `packages/tddy-web/docs/session-connections.md`
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change` (req 2, 3)
 */

import { describe, it, expect } from "bun:test";
import type { Client, Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import type { TerminalFeed } from "../../rpc/connections/terminal";
import type { SessionConnection } from "../../rpc/connections/session";
import type { ConnectionCapability } from "../../rpc/connections/types";
import {
  SessionRuntimeRegistry,
  makeByteTap,
  type SessionRuntimeState,
} from "./sessionRuntimeRegistry";

function aRuntimeState(sessionId: string): SessionRuntimeState {
  return {
    sessionId,
    attached: true,
    bytesIn: 0,
    bytesOut: 0,
    lastDataReceivedAt: null,
  };
}

/** A controllable clock injected into the byte tap so `at` timestamps are deterministic. */
function aClockAt(ms: number) {
  let value = ms;
  return {
    now: () => value,
    advanceTo(next: number) {
      value = next;
    },
  };
}

describe("SessionRuntimeRegistry", () => {
  it("keeps a backgrounded session's runtime after a focus switch and only evicts on explicit disconnect", () => {
    // Given — two attached sessions with A focused
    const registry = new SessionRuntimeRegistry();
    registry.add("session-a", aRuntimeState("session-a"));
    registry.add("session-b", aRuntimeState("session-b"));
    registry.focus("session-a");
    expect(registry.focusedSessionId).toBe("session-a");

    // When — the user switches focus to B
    registry.focus("session-b");

    // Then — A is still mounted (not evicted) and B is focused
    expect(registry.focusedSessionId).toBe("session-b");
    expect(registry.get("session-a")?.attached).toBe(true);
    expect(registry.get("session-b")?.attached).toBe(true);
    expect(registry.runtimes.map((r) => r.sessionId).sort()).toEqual(["session-a", "session-b"]);

    // When — the user explicitly disconnects A
    registry.disconnect("session-a");

    // Then — only A is evicted; B remains
    expect(registry.get("session-a")).toBeUndefined();
    expect(registry.get("session-b")?.attached).toBe(true);
  });

  it("notifies subscribers when a runtime is added, focused, or disconnected", () => {
    // Given
    const registry = new SessionRuntimeRegistry();
    const events: string[] = [];
    registry.subscribe(() => events.push("notify"));

    // When
    registry.add("session-a", aRuntimeState("session-a"));
    registry.focus("session-a");
    registry.disconnect("session-a");

    // Then — one notification per mutation
    expect(events).toEqual(["notify", "notify", "notify"]);
    expect(registry.runtimes).toHaveLength(0);
  });

  it("updates byte counters and lastDataReceivedAt on a background runtime without refocusing it", () => {
    // Given — A is focused, B is backgrounded
    const registry = new SessionRuntimeRegistry();
    registry.add("session-a", aRuntimeState("session-a"));
    registry.add("session-b", aRuntimeState("session-b"));
    registry.focus("session-a");

    // When — bytes arrive for the backgrounded B
    registry.recordBytes("session-b", { bytesIn: 128, bytesOut: 32, at: 1_700_000_000_000 });

    // Then — B's counters update while focus stays on A
    expect(registry.focusedSessionId).toBe("session-a");
    expect(registry.get("session-b")?.bytesIn).toBe(128);
    expect(registry.get("session-b")?.bytesOut).toBe(32);
    expect(registry.get("session-b")?.lastDataReceivedAt).toBe(1_700_000_000_000);
  });

  it("byte tap folds successive inbound chunks into cumulative bytesIn and stamps last-received from the injected clock", () => {
    // Given — an attached runtime and a byte tap bound to it via a controllable clock. This is the
    // sink the terminal fires for each output chunk (bytesIn = output.data.length).
    const registry = new SessionRuntimeRegistry();
    registry.add("session-a", aRuntimeState("session-a"));
    const clock = aClockAt(1_700_000_000_000);
    const tap = makeByteTap(registry, "session-a", clock.now);

    // When — two output chunks arrive, the second a beat later
    tap({ bytesIn: 60 });
    clock.advanceTo(1_700_000_005_000);
    tap({ bytesIn: 40 });

    // Then — bytesIn accumulates and last-received reflects the latest chunk's clock reading
    expect(registry.get("session-a")?.bytesIn).toBe(100);
    expect(registry.get("session-a")?.bytesOut).toBe(0);
    expect(registry.get("session-a")?.lastDataReceivedAt).toBe(1_700_000_005_000);
  });

  it("byte tap folds outbound input yields into cumulative bytesOut without inflating bytesIn", () => {
    // Given — an attached runtime and a byte tap. This is the sink the terminal fires for each
    // batched input yield (bytesOut = data.length).
    const registry = new SessionRuntimeRegistry();
    registry.add("session-a", aRuntimeState("session-a"));
    const tap = makeByteTap(registry, "session-a", aClockAt(1_700_000_000_000).now);

    // When — two batched input yields are sent to the coder
    tap({ bytesOut: 12 });
    tap({ bytesOut: 8 });

    // Then — bytesOut accumulates while bytesIn stays zero
    expect(registry.get("session-a")?.bytesOut).toBe(20);
    expect(registry.get("session-a")?.bytesIn).toBe(0);
    expect(registry.get("session-a")?.lastDataReceivedAt).toBe(null);
  });
});

const A_SESSION = "session-0001";
const ANOTHER_SESSION = "session-0002";

/**
 * A session connection that records whether anybody released it.
 *
 * `closes` counts rather than latches, because "closed once" and "closed twice" are different
 * claims: a re-attach that closed the connection it is installing would look identical to one that
 * closed the connection it replaced.
 */
function aSessionConnection(sessionId: string) {
  let closes = 0;
  const connection: SessionConnection = {
    hostId: "local",
    sessionId,
    status: "connected",
    error: null,
    capabilities: new Set<ConnectionCapability>(["rpc"]),
    clientFor: <S extends DescService>(): Client<S> => {
      throw new Error("this connection is a lifetime stand-in and issues no calls");
    },
    transport: (): Transport => {
      throw new Error("this connection is a lifetime stand-in and issues no calls");
    },
    openTerminal: (): TerminalFeed => {
      throw new Error("this connection is a lifetime stand-in and serves no terminal");
    },
    close: () => {
      closes += 1;
    },
  };
  return { connection, closes: () => closes };
}

/** An attached runtime holding `connection` — what the drawer stores when an attach resolves. */
function aRuntimeHolding(sessionId: string, connection: SessionConnection): SessionRuntimeState {
  return {
    sessionId,
    attached: true,
    connection,
    hint: { sessionId },
    bytesIn: 0,
    bytesOut: 0,
    lastDataReceivedAt: null,
  };
}

describe("a runtime's session connection", () => {
  it("is released when another runtime takes its place", () => {
    // Given a mounted runtime holding a connection
    const registry = new SessionRuntimeRegistry();
    const first = aSessionConnection(A_SESSION);
    registry.add(A_SESSION, aRuntimeHolding(A_SESSION, first.connection));

    // When the same session is re-attached, wholesale
    registry.add(A_SESSION, aRuntimeHolding(A_SESSION, aSessionConnection(A_SESSION).connection));

    // Then the connection that was displaced is released. Without this every re-attach would leave
    // a joined room nobody will ever disconnect
    expect(first.closes()).toEqual(1);
  });

  it("is released when a re-attach patches a different one in", () => {
    // Given a mounted runtime holding a connection
    const registry = new SessionRuntimeRegistry();
    const first = aSessionConnection(A_SESSION);
    registry.add(A_SESSION, aRuntimeHolding(A_SESSION, first.connection));

    // When the attach resolves to a genuinely new connection
    const second = aSessionConnection(A_SESSION);
    registry.updateConnection(A_SESSION, {
      connection: second.connection,
      hint: { sessionId: A_SESSION },
    });

    // Then the one it replaces goes, and the one it installs stays
    expect(first.closes()).toEqual(1);
    expect(second.closes()).toEqual(0);
  });

  it("survives being re-installed over itself", () => {
    // Given a mounted runtime holding a connection
    const registry = new SessionRuntimeRegistry();
    const held = aSessionConnection(A_SESSION);
    registry.add(A_SESSION, aRuntimeHolding(A_SESSION, held.connection));

    // When the drawer's fast-path select restores the attachment it already had — the same object,
    // handed straight back
    registry.updateConnection(A_SESSION, {
      connection: held.connection,
      hint: { sessionId: A_SESSION },
    });

    // Then it is left alone. Closing here would detach the session the operator just selected,
    // which is the whole point of a fast path that avoids an RPC round-trip
    expect(held.closes()).toEqual(0);
  });

  it("is released when its runtime is explicitly disconnected", () => {
    // Given a mounted runtime
    const registry = new SessionRuntimeRegistry();
    const held = aSessionConnection(A_SESSION);
    registry.add(A_SESSION, aRuntimeHolding(A_SESSION, held.connection));

    // When the session is disconnected
    registry.disconnect(A_SESSION);

    // Then the connection is released and the runtime is gone
    expect(held.closes()).toEqual(1);
    expect(registry.get(A_SESSION)).toBeUndefined();
  });

  it("is released, along with every other, when the owning screen goes", () => {
    // Given two sessions attached at once — two open terminals in the drawer
    const registry = new SessionRuntimeRegistry();
    const one = aSessionConnection(A_SESSION);
    const other = aSessionConnection(ANOTHER_SESSION);
    registry.add(A_SESSION, aRuntimeHolding(A_SESSION, one.connection));
    registry.add(ANOTHER_SESSION, aRuntimeHolding(ANOTHER_SESSION, other.connection));

    // When the screen holding the registry unmounts
    registry.closeAll();

    // Then every connection is released and the store is empty. The registry lives in the screen's
    // own ref, so anything still held here is held by nothing at all a moment later
    expect(one.closes()).toEqual(1);
    expect(other.closes()).toEqual(1);
    expect(registry.runtimes).toEqual([]);
  });

  it("is not released twice when the screen unmounts after a disconnect", () => {
    // Given a session that was already disconnected by hand
    const registry = new SessionRuntimeRegistry();
    const held = aSessionConnection(A_SESSION);
    registry.add(A_SESSION, aRuntimeHolding(A_SESSION, held.connection));
    registry.disconnect(A_SESSION);

    // When the screen then unmounts
    registry.closeAll();

    // Then the connection saw exactly one release — an evicted runtime is no longer the registry's
    // to close a second time
    expect(held.closes()).toEqual(1);
  });
});
