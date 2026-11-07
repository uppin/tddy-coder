/**
 * The host-served session opener `tddy-web` hands to `LiveKitConnectionProvider` (and to the local
 * host): a session whose attach reply names no room is reached over the host connection itself.
 *
 * `tddy-livekit-web`'s provider specs assert only that the provider hands its host to this opener;
 * these pin what the opener does with it.
 */
import { describe, expect, it } from "bun:test";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { aClientsPortDouble } from "tddy-client-web/testkit";
import { attachmentHintFromReply, type HostConnection } from "tddy-client-web";
import { SessionService } from "tddy-client-web/gen/session_pb";
import { openHostServedSession } from "./hostServedSession";

const A_SESSION = "session-0001";
const A_HOST_SERVED_REPLY = { livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" };

function aConnectedHost(): HostConnection {
  const host = aClientsPortDouble(anInMemoryRpcBackend()).connections.connectHost("host-a");
  if (host === null) throw new Error("the clients double lists host-a");
  return host;
}

function aHostServedSessionOn(host: HostConnection) {
  return openHostServedSession(host, attachmentHintFromReply(A_SESSION, A_HOST_SERVED_REPLY));
}

describe("a host-served session", () => {
  it("is reached with the host's own client, not a second one", () => {
    // Given a session its host answers itself
    const host = aConnectedHost();
    const session = aHostServedSessionOn(host);

    // When a client is asked for
    const client = session.clientFor(SessionService);

    // Then it is the host's — identity holds for as long as the host connection does
    expect(client).toBe(host.clientFor(SessionService));
  });

  it("advertises rpc only, so the media surfaces do not apply to it", () => {
    // Given / When
    const session = aHostServedSessionOn(aConnectedHost());

    // Then nothing a room carries is on offer
    expect([...session.capabilities]).toEqual(["rpc"]);
  });

  it("reads its status through the host serving it", () => {
    // Given / When
    const session = aHostServedSessionOn(aConnectedHost());

    // Then a session is as reachable as its host, and names the host and session it addresses
    expect(session.status).toEqual("connected");
    expect(session.hostId).toEqual("host-a");
    expect(session.sessionId).toEqual(A_SESSION);
  });

  it("refuses calls once closed, and reports idle rather than a host fault", () => {
    // Given an open session
    const session = aHostServedSessionOn(aConnectedHost());

    // When it is detached
    session.close();

    // Then nothing is asked of it any more, and a late call throws instead of quietly succeeding
    expect(session.status).toEqual("idle");
    expect(session.error).toBeNull();
    expect(() => session.clientFor(SessionService)).toThrow(`session ${A_SESSION} on host host-a is closed`);
    expect(() => session.transport()).toThrow(`session ${A_SESSION} on host host-a is closed`);
  });
});
