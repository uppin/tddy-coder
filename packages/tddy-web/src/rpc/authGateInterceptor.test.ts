/**
 * Unit tests for createAuthGateInterceptor — the ConnectRPC interceptor that gates every RPC
 * behind a request-time-fresh access token. Before forwarding a request whose message carries a
 * `sessionToken` field, it awaits the injected token resolver (single-flight refresh lives in the
 * resolver) and rewrites the field, so a call issued right after waking waits for the refresh and
 * is sent with the fresh token rather than the stale one.
 *
 * Changeset: `durable-web-session`
 * PRD: `docs/ft/daemon/1-WIP/PRD-2026-07-04-durable-web-session.md`
 */

import { describe, it, expect } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { SessionService, ListSessionsRequestSchema } from "../gen/session_pb";
import { AuthService, GetAuthUrlRequestSchema } from "../gen/auth_pb";
import { ActivityService, StreamAcpReplayRequestSchema } from "../gen/activity_pb";

import { createAuthGateInterceptor } from "./authGateInterceptor";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function aListSessionsRequest(sessionToken: string) {
  return {
    stream: false as const,
    service: SessionService,
    method: SessionService.method.listSessions,
    message: create(ListSessionsRequestSchema, { sessionToken }),
    header: new Headers(),
    url: "/rpc/session.SessionService/ListSessions",
    init: {},
    signal: new AbortController().signal,
  };
}


/** A server-streaming request, as ConnectRPC hands one to an interceptor: `stream: true` and a
 *  `message` that is an AsyncIterable of the outgoing request messages. */
function aStreamAcpReplayRequest(sessionToken: string) {
  return {
    stream: true as const,
    service: ActivityService,
    method: ActivityService.method.streamAcpReplay,
    message: (async function* () {
      yield create(StreamAcpReplayRequestSchema, { sessionToken, sessionId: "s-1" });
    })(),
    header: new Headers(),
    url: "/rpc/activity.ActivityService/StreamAcpReplay",
    init: {},
    signal: new AbortController().signal,
  };
}

function aResponseFor(req: { service: unknown; method: unknown; message: unknown }) {
  return {
    stream: false as const,
    service: req.service,
    method: req.method,
    message: req.message,
    header: new Headers(),
    trailer: new Headers(),
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("createAuthGateInterceptor", () => {
  it("rewrites the request sessionToken field with a freshly resolved access token", async () => {
    // Given — a resolver that yields a fresh token, and a request carrying a stale one
    const interceptor = createAuthGateInterceptor(async () => "fresh-access-token");
    const req = aListSessionsRequest("stale-token");
    const next = async (r: typeof req) => aResponseFor(r);

    // When — the gated request is sent
    await interceptor(next as never)(req as never);

    // Then — the outgoing message carries the fresh token, not the stale one
    expect((req.message as { sessionToken: string }).sessionToken).toBe("fresh-access-token");
  });

  it("waits for an in-flight refresh before forwarding the request", async () => {
    // Given — a resolver whose refresh has not yet completed
    let resolveToken: (t: string) => void = () => {};
    const pending = new Promise<string>((resolve) => {
      resolveToken = resolve;
    });
    const interceptor = createAuthGateInterceptor(() => pending);
    const req = aListSessionsRequest("stale-token");
    let forwarded = false;
    const next = async (r: typeof req) => {
      forwarded = true;
      return aResponseFor(r);
    };

    // When — the request is sent while the refresh is still in flight
    const call = interceptor(next as never)(req as never);
    await Promise.resolve();

    // Then — it has not been forwarded yet
    expect(forwarded).toBe(false);

    // When — the refresh completes
    resolveToken("fresh-access-token");
    await call;

    // Then — the request is forwarded with the fresh token
    expect(forwarded).toBe(true);
    expect((req.message as { sessionToken: string }).sessionToken).toBe("fresh-access-token");
  });

  it("leaves a request without a sessionToken field untouched", async () => {
    // Given — a request whose message has no sessionToken (GetAuthUrl)
    const interceptor = createAuthGateInterceptor(async () => "fresh-access-token");
    const req = {
      stream: false as const,
      service: AuthService,
      method: AuthService.method.getAuthUrl,
      message: create(GetAuthUrlRequestSchema, {}),
      header: new Headers(),
      url: "/rpc/auth.AuthService/GetAuthUrl",
      init: {},
      signal: new AbortController().signal,
    };
    let forwarded = false;
    const next = async (r: typeof req) => {
      forwarded = true;
      return aResponseFor(r);
    };

    // When — it is sent through the gate
    await interceptor(next as never)(req as never);

    // Then — it is forwarded unchanged, gaining no sessionToken field
    expect(forwarded).toBe(true);
    expect("sessionToken" in req.message).toBe(false);
  });

  it("rewrites the sessionToken on a streaming request too, so a stream is not sent with a stale token", async () => {
    // Given — a server-streaming call (StreamAcpReplay) carrying a token that has since lapsed
    const interceptor = createAuthGateInterceptor(async () => "fresh-access-token");
    const req = aStreamAcpReplayRequest("stale-token");
    const sent: { sessionToken: string }[] = [];
    const next = async (r: { message: AsyncIterable<{ sessionToken: string }> }) => {
      // The transport is what drains the outgoing iterable; do the same so the gate's rewrite runs.
      for await (const m of r.message) sent.push(m);
      return aResponseFor(req as never);
    };

    // When — the stream is opened through the gate
    await interceptor(next as never)(req as never);

    // Then — the message that actually went out carries the fresh token, not the stale one
    expect(sent).toHaveLength(1);
    expect(sent[0]!.sessionToken).toBe("fresh-access-token");
  });
});
