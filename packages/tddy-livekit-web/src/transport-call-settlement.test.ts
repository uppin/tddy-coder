/**
 * Every call this transport starts must eventually settle.
 *
 * Today only `unary` can: it honours its abort signal and its `timeoutMs`. The three streaming paths
 * take `_signal` and never read it, and `stream()` receives `_timeoutMs` and never forwards it, so a
 * streaming call has no way to end except a frame arriving from the peer. Nothing settles a call
 * when the peer leaves the room — the registry listens only to `DataReceived` — and `publishData` is
 * fire-and-forget, so a call whose request never went out doesn't fail either. `dispose()` drops the
 * pending maps with `clear()` rather than failing them, orphaning every awaiter.
 *
 * The practical cost is not the disconnect case but the abort one: `useTerminalControl` and
 * `GrpcSessionTerminal` both abort on effect cleanup, so every session switch and every re-subscribe
 * leaves a `pendingStreams` entry alive for the life of the page.
 *
 * The rule these tests pin: **voluntary cancellation ends a stream; involuntary failure errors it.**
 * A caller that aborted its own call has nothing to be told, and every consumer already treats
 * stream-end as normal — whereas a deadline, a departed peer or a request that never left are
 * failures the caller cannot otherwise discover.
 */

import { describe, it, expect } from "bun:test";
import { Code, ConnectError } from "@connectrpc/connect";
import { create, toBinary } from "@bufbuild/protobuf";
import { RoomEvent } from "livekit-client";
import { RpcRequestSchema, RpcResponseSchema } from "./gen/rpc_envelope_pb.js";
import { LiveKitTransport, RoomRpcRegistry } from "./transport.js";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const A_DAEMON = "daemon-udoo";
const ANOTHER_DAEMON = "daemon-pi";

const A_UNARY_METHOD = {
  kind: "unary",
  methodKind: "unary",
  name: "ClaimTerminalControl",
  parent: { typeName: "connection.ConnectionService" },
  input: RpcRequestSchema,
  output: RpcResponseSchema,
} as any;

const A_SERVER_STREAMING_METHOD = {
  kind: "rpc",
  methodKind: "server_streaming",
  name: "WatchTerminalControl",
  parent: { typeName: "connection.ConnectionService" },
  input: RpcRequestSchema,
  output: RpcResponseSchema,
} as any;

const A_BIDI_METHOD = {
  kind: "rpc",
  methodKind: "bidi_streaming",
  name: "StreamSessionTerminalIO",
  parent: { typeName: "connection.ConnectionService" },
  input: RpcRequestSchema,
  output: RpcResponseSchema,
} as any;

/** One request message, enough to open any of the streaming kinds. */
async function* oneRequest() {
  yield create(RpcRequestSchema, { requestId: 0 });
}

// ---------------------------------------------------------------------------
// Outcome helpers
//
// A call that never settles is the defect under test, so every wait is bounded: the helpers report
// "still pending" instead of hanging the suite, which turns the failure into a readable assertion.
// ---------------------------------------------------------------------------

const STILL_PENDING = "still-pending";
const ENDED_CLEANLY = "ended-cleanly";
const SETTLE_BUDGET_MS = 250;

/** How `call` settled: its rejection, `"resolved"`, or `STILL_PENDING`. */
async function outcomeOf(call: Promise<unknown>): Promise<unknown> {
  return Promise.race([
    call.then(
      () => "resolved",
      (error) => error,
    ),
    new Promise((resolve) => setTimeout(() => resolve(STILL_PENDING), SETTLE_BUDGET_MS)),
  ]);
}

/** How the stream behind `open` ended: its thrown error, `ENDED_CLEANLY`, or `STILL_PENDING`. */
async function streamOutcomeOf(open: Promise<{ message: AsyncIterable<unknown> }>): Promise<unknown> {
  const drained = (async () => {
    const response = await open;
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    for await (const _frame of response.message) {
      // Frames are irrelevant here; these tests are about how the stream *ends*.
    }
    return ENDED_CLEANLY;
  })();
  return Promise.race([
    drained.then(
      (ended) => ended,
      (error) => error,
    ),
    new Promise((resolve) => setTimeout(() => resolve(STILL_PENDING), SETTLE_BUDGET_MS)),
  ]);
}

// ---------------------------------------------------------------------------
// Fake room
// ---------------------------------------------------------------------------

type AnyListener = (...args: unknown[]) => void;

function aRoom(options: { publishFails?: boolean } = {}) {
  const listeners: Record<string, AnyListener[]> = {};
  return {
    on(event: string, handler: AnyListener) {
      (listeners[event] ??= []).push(handler);
      return this;
    },
    off(event: string, handler: AnyListener) {
      const handlers = listeners[event] ?? [];
      const at = handlers.indexOf(handler);
      if (at >= 0) handlers.splice(at, 1);
      return this;
    },
    localParticipant: {
      identity: "web-alice-1755100800000-k3f9qz",
      /** Real `publishData` is async and rejects on a room that has gone away. */
      publishData(): Promise<void> {
        return options.publishFails
          ? Promise.reject(new Error("could not publish: room disconnected"))
          : Promise.resolve();
      },
    },
    /** The room reporting that a remote participant left. */
    emitParticipantLeft(identity: string) {
      for (const handler of (listeners[RoomEvent.ParticipantDisconnected] ?? []).slice()) {
        handler({ identity });
      }
    },
  };
}

// ---------------------------------------------------------------------------
// Fluent driver
// ---------------------------------------------------------------------------

function aBrowser(options: { publishFails?: boolean } = {}) {
  const room = aRoom(options);
  const registry = new RoomRpcRegistry(room as never, false);
  const transportTo = (target: string) =>
    new LiveKitTransport({ room: room as never, targetIdentity: target, registry });

  return {
    /** Start a unary call to `target` and hand back its pending promise. */
    callsUnaryTo(target: string) {
      return transportTo(target).unary(A_UNARY_METHOD, undefined, undefined, undefined, {});
    },
    /** Open a server-streaming call to `target`. */
    opensStreamTo(
      target: string,
      opts: { signal?: AbortSignal; timeoutMs?: number } = {},
    ): Promise<{ message: AsyncIterable<unknown> }> {
      return transportTo(target).stream(
        A_SERVER_STREAMING_METHOD,
        opts.signal,
        opts.timeoutMs,
        undefined,
        oneRequest() as never,
      ) as unknown as Promise<{ message: AsyncIterable<unknown> }>;
    },
    /** Open a bidi call to `target`. */
    opensBidiTo(
      target: string,
      opts: { signal?: AbortSignal } = {},
    ): Promise<{ message: AsyncIterable<unknown> }> {
      return transportTo(target).stream(
        A_BIDI_METHOD,
        opts.signal,
        undefined,
        undefined,
        oneRequest() as never,
      ) as unknown as Promise<{ message: AsyncIterable<unknown> }>;
    },
    /** The room reports that `target` left. */
    daemonLeaves(target: string) {
      room.emitParticipantLeft(target);
    },
    /** Tear the room's correlation state down, as a room teardown does. */
    disposeRegistry() {
      registry.dispose();
    },
    /** How many calls the registry still holds — a call that is done must leave nothing behind. */
    pendingCallCount() {
      return registry.pendingUnary.size + registry.pendingStreams.size;
    },
  };
}

/** An `AbortController` whose signal has already fired, as an unmounted effect's would have. */
function anAbortedSignal(): AbortSignal {
  const controller = new AbortController();
  controller.abort();
  return controller.signal;
}

function expectConnectError(outcome: unknown, code: Code): void {
  expect(outcome).toBeInstanceOf(ConnectError);
  expect((outcome as ConnectError).code).toBe(code);
}

// ---------------------------------------------------------------------------
// Voluntary cancellation: the stream ends, and leaves nothing behind.
// ---------------------------------------------------------------------------

describe("a cancelled streaming call", () => {
  it("ends the response when its abort signal fires", async () => {
    // Given — a browser and an effect that has already been torn down
    const browser = aBrowser();

    // When — the stream is opened with that effect's spent signal
    const outcome = await streamOutcomeOf(
      browser.opensStreamTo(A_DAEMON, { signal: anAbortedSignal() }),
    );

    // Then — the response ends rather than waiting on a peer that will never answer
    expect(outcome).toBe(ENDED_CLEANLY);
  });

  it("releases its registration when its abort signal fires", async () => {
    // Given — a stream opened by an effect that has already been torn down
    const browser = aBrowser();
    await streamOutcomeOf(browser.opensStreamTo(A_DAEMON, { signal: anAbortedSignal() }));

    // Then — nothing is retained. Every session switch aborts and re-subscribes, so a registration
    // that outlives its call accumulates for the life of the page.
    expect(browser.pendingCallCount()).toBe(0);
  });

  it("ends a bidi response when its abort signal fires", async () => {
    // Given
    const browser = aBrowser();

    // When
    const outcome = await streamOutcomeOf(
      browser.opensBidiTo(A_DAEMON, { signal: anAbortedSignal() }),
    );

    // Then
    expect(outcome).toBe(ENDED_CLEANLY);
  });
});

// ---------------------------------------------------------------------------
// Involuntary failure: the call errors, with a code that says why.
// ---------------------------------------------------------------------------

describe("a streaming call that outlives its deadline", () => {
  it("fails with deadline_exceeded when no frame arrives in time", async () => {
    // Given — a browser, and a stream opened with a deadline shorter than the settle budget so the
    // deadline is what settles the call rather than the test's own bound
    const browser = aBrowser();

    // When
    const outcome = await streamOutcomeOf(browser.opensStreamTo(A_DAEMON, { timeoutMs: 50 }));

    // Then — `stream()` currently drops `timeoutMs` before any handler sees it
    expectConnectError(outcome, Code.DeadlineExceeded);
  });
});

describe("a call whose daemon leaves the room", () => {
  it("fails an in-flight unary call with unavailable", async () => {
    // Given — a claim in flight to a daemon
    const browser = aBrowser();
    const call = browser.callsUnaryTo(A_DAEMON);

    // When — that daemon leaves
    browser.daemonLeaves(A_DAEMON);

    // Then — the caller is told, instead of awaiting a response that can never come
    expectConnectError(await outcomeOf(call), Code.Unavailable);
  });

  it("fails an in-flight stream with unavailable", async () => {
    // Given — a control watch open against a daemon
    const browser = aBrowser();
    const open = browser.opensStreamTo(A_DAEMON);
    const outcome = streamOutcomeOf(open);
    await open;

    // When — that daemon leaves
    browser.daemonLeaves(A_DAEMON);

    // Then
    expectConnectError(await outcome, Code.Unavailable);
  });

  it("leaves a call to another daemon in flight", async () => {
    // Given — one call to each of two daemons over the browser's single shared registry
    const browser = aBrowser();
    const toTheDepartingDaemon = browser.callsUnaryTo(A_DAEMON);
    const toTheStayingDaemon = browser.callsUnaryTo(ANOTHER_DAEMON);

    // When — only one of them leaves
    browser.daemonLeaves(A_DAEMON);

    // Then — the other call is untouched. The registry keys pending calls by request id alone, so
    // failing them without regard to their target would take this one down too.
    expectConnectError(await outcomeOf(toTheDepartingDaemon), Code.Unavailable);
    expect(await outcomeOf(toTheStayingDaemon)).toBe(STILL_PENDING);
  });
});

describe("a call whose request never went out", () => {
  it("fails with unavailable when publishing the request fails", async () => {
    // Given — a room that can no longer publish
    const browser = aBrowser({ publishFails: true });

    // When
    const outcome = await outcomeOf(browser.callsUnaryTo(A_DAEMON));

    // Then — `publishData` is currently fire-and-forget, so the rejection is unobserved and the
    // call waits for a response to a request that was never sent
    expectConnectError(outcome, Code.Unavailable);
  });
});

describe("a registry that is disposed", () => {
  it("fails the calls still in flight", async () => {
    // Given — a call in flight
    const browser = aBrowser();
    const call = browser.callsUnaryTo(A_DAEMON);

    // When — the room's correlation state is torn down
    browser.disposeRegistry();

    // Then — `dispose` currently drops the pending maps with `clear()`, orphaning the awaiter
    expectConnectError(await outcomeOf(call), Code.Canceled);
  });
});
