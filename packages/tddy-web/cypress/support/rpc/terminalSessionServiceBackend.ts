/**
 * In-memory `terminal_session.TerminalSessionService` backend — a session's terminals, their byte
 * streams, their scrollback and the control lease that decides who may type into them.
 *
 * Split out of `daemonSessionHostBackend.ts` when the nine terminal RPCs left
 * `session.SessionService` for `terminal_session.TerminalSessionService`: a fake is registered
 * per service, so a screen's backend composes the two rather than one `.implement` covering both.
 * `aTerminalSessionServiceFake` is the composable half (`handlers` spread into a caller's own
 * `.implement(TerminalSessionService, …)`), `aTerminalSessionServiceBackend` the standalone one,
 * mirroring `hostServiceBackend.ts`.
 */

import { create } from "@bufbuild/protobuf";
import type { ServiceImpl } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ClaimTerminalControlResponseSchema,
  ListTerminalSessionsResponseSchema,
  SessionTerminalOutputSchema,
  StartTerminalSessionResponseSchema,
  StopTerminalSessionResponseSchema,
  TerminalControlEventSchema,
  TerminalHistoryChunkSchema,
  TerminalSessionInfoSchema,
  TerminalSessionService,
} from "../../../src/gen/terminal_session_pb";

export interface TerminalSessionServiceScenario {
  /** Initial `ListTerminalSessions` result — the bash terminals already open on the session.
   *  Defaults to none (only the reserved "main"/Agent terminal, which is not listed here). */
  terminals?: Array<{ terminalId: string; kind?: string; pid?: number }>;
  /** The `terminal_id` handed out by the Nth (0-based) `StartTerminalSession`. Default `bash-<n+1>`. */
  newTerminalId?: (index: number) => string;
  /** Absolute `endOffset` carried by the initial `StreamTerminalOutput` replay frame — the anchor
   *  for lazy scroll-up history. When set (> 0n), the backend's `streamTerminalOutput` emits its
   *  identifying frame tagged with this offset (and `atOldest`), mirroring the daemon's lazy
   *  replay. Default: 0n (no offset metadata — legacy shape). */
  terminalReplayEndOffset?: bigint;
  /** How many `StreamTerminalOutput` subscriptions **end** after their first frame instead of
   *  staying open — the daemon dropping a terminal feed (a closed data-channel stream, a `pty_done`).
   *  The first `n` opened streams drop; every later one stays open, so a test can drop a feed once
   *  and still observe what the screen does to recover. Default 0 (no stream ever ends). */
  droppedTerminalStreams?: number;
  /** Older history chunks returned by `GetTerminalHistory`, in the order they should be yielded
   *  across successive calls (one chunk per call). Each chunk's `startOffset`/`endOffset`/`atOldest`
   *  is used verbatim. The backend pops one chunk per `getTerminalHistory` call. */
  terminalHistory?: Array<{ data: Uint8Array; startOffset: bigint; endOffset: bigint; atOldest: boolean }>;
}

export interface TerminalSessionServiceControls {
  /** Every `sessionId` passed to `ClaimTerminalControl`, in call order. */
  readonly claimedControlSessionIds: string[];
  /** Every `sessionId` passed to `StartTerminalSession`, in call order. */
  readonly startTerminalSessionIds: string[];
  /** The `terminal_id` handed back by each `StartTerminalSession`, in call order. */
  readonly startedTerminalIds: string[];
  /** Every `{ sessionId, terminalId }` passed to `StopTerminalSession`, in call order. */
  readonly stoppedTerminals: { sessionId: string; terminalId: string }[];
  /** Every `{ sessionId, terminalId, data }` passed to `SendTerminalInput`, in call order. */
  readonly sentTerminalInput: { sessionId: string; terminalId: string; data: Uint8Array }[];
  /** Every `{ sessionId, terminalId }` an output stream was opened for, in call order. */
  readonly streamedTerminals: { sessionId: string; terminalId: string }[];
  /** Every `{ sessionId, terminalId, fromOffset, untilOffset }` passed to `GetTerminalHistory`, in
   *  call order — the forward fill's anchor chain. */
  readonly getTerminalHistoryCalls: {
    sessionId: string;
    terminalId: string;
    fromOffset: bigint;
    untilOffset: bigint;
  }[];
}

/** The terminal fake as handlers, so a screen's own backend can serve it too. */
export interface TerminalSessionServiceFake extends TerminalSessionServiceControls {
  handlers: Partial<ServiceImpl<typeof TerminalSessionService>>;
}

/**
 * Build the `terminal_session.TerminalSessionService` handlers for `scenario`, plus the recorders a
 * spec asserts on.
 */
export function aTerminalSessionServiceFake(
  scenario: TerminalSessionServiceScenario = {},
): TerminalSessionServiceFake {
  const claimedControlSessionIds: string[] = [];
  const startTerminalSessionIds: string[] = [];
  const startedTerminalIds: string[] = [];
  const stoppedTerminals: { sessionId: string; terminalId: string }[] = [];
  const sentTerminalInput: { sessionId: string; terminalId: string; data: Uint8Array }[] = [];
  const streamedTerminals: { sessionId: string; terminalId: string }[] = [];
  const getTerminalHistoryCalls: {
    sessionId: string;
    terminalId: string;
    fromOffset: bigint;
    untilOffset: bigint;
  }[] = [];

  // Live bash-terminal list — mutated by Start/Stop so ListTerminalSessions stays consistent.
  const liveTerminals: { terminalId: string; kind: string; pid: number }[] = (
    scenario.terminals ?? []
  ).map((t, i) => ({ terminalId: t.terminalId, kind: t.kind ?? "bash", pid: t.pid ?? 8000 + i }));
  const nextTerminalId = scenario.newTerminalId ?? ((index: number) => `bash-${index + 1}`);

  const handlers: Partial<ServiceImpl<typeof TerminalSessionService>> = {
    claimTerminalControl: async (req) => {
      claimedControlSessionIds.push(req.sessionId);
      return create(ClaimTerminalControlResponseSchema, { granted: true, controlToken: "ctrl-1" });
    },
    // The daemon's on-subscribe snapshot of the lease, then the stream completes. Its zero values
    // say "no screen is named as the holder" — the claim above is what grants control; this feed
    // only reports what the daemon holds.
    watchTerminalControl: async function* () {
      yield create(TerminalControlEventSchema, {});
    },
    // --- Multiple terminals per session ---
    listTerminalSessions: async () =>
      create(ListTerminalSessionsResponseSchema, {
        terminals: liveTerminals.map((t) => create(TerminalSessionInfoSchema, t)),
      }),
    startTerminalSession: async (req) => {
      const terminalId = nextTerminalId(startTerminalSessionIds.length);
      startTerminalSessionIds.push(req.sessionId);
      startedTerminalIds.push(terminalId);
      liveTerminals.push({ terminalId, kind: "bash", pid: 8000 + liveTerminals.length });
      return create(StartTerminalSessionResponseSchema, { terminalId });
    },
    stopTerminalSession: async (req) => {
      stoppedTerminals.push({ sessionId: req.sessionId, terminalId: req.terminalId });
      const at = liveTerminals.findIndex((t) => t.terminalId === req.terminalId);
      if (at !== -1) liveTerminals.splice(at, 1);
      return create(StopTerminalSessionResponseSchema, { ok: true, message: "" });
    },
    sendTerminalInput: async (req) => {
      sentTerminalInput.push({
        sessionId: req.sessionId,
        terminalId: req.terminalId,
        data: req.data,
      });
      return {};
    },
    // Server-streaming output — record the opened stream, emit one identifying frame, then stay
    // open (a terminal stream that *completes* would signal disconnect and evict the runtime).
    // When `scenario.terminalReplayEndOffset` is set, the frame is tagged with the absolute
    // `endOffset` + `atOldest` so the lazy scroll-up loader can anchor older-history fetches.
    // The frame carries the session and RESOLVED terminal id it came from, as the daemon stamps
    // every frame — a pane drops frames that are not its own.
    // `scenario.droppedTerminalStreams` makes the first n subscriptions *return* after that frame,
    // which is how the daemon dropping the feed reaches the browser.
    streamTerminalOutput: async function* (req) {
      const openIndex = streamedTerminals.length;
      streamedTerminals.push({ sessionId: req.sessionId, terminalId: req.terminalId });
      const terminalId = req.terminalId || "main";
      yield create(SessionTerminalOutputSchema, {
        data: new TextEncoder().encode(`term:${terminalId}\r\n`),
        endOffset: scenario.terminalReplayEndOffset ?? 0n,
        atOldest: (scenario.terminalHistory ?? []).length === 0,
        sessionId: req.sessionId,
        terminalId,
      });
      if (openIndex < (scenario.droppedTerminalStreams ?? 0)) return;
      await new Promise<never>(() => undefined);
    },
    // Lazy scroll-up history — yield one chunk per call from `scenario.terminalHistory` (popped
    // in order), so a test can assert the loader chains anchors across successive calls.
    getTerminalHistory: async function* (req) {
      getTerminalHistoryCalls.push({
        sessionId: req.sessionId,
        terminalId: req.terminalId,
        fromOffset: req.fromOffset,
        untilOffset: req.untilOffset,
      });
      const chunk = (scenario.terminalHistory ?? []).shift();
      if (chunk) {
        yield create(TerminalHistoryChunkSchema, chunk);
      }
    },
  };

  return {
    handlers,
    claimedControlSessionIds,
    startTerminalSessionIds,
    startedTerminalIds,
    stoppedTerminals,
    sentTerminalInput,
    streamedTerminals,
    getTerminalHistoryCalls,
  };
}

export interface TerminalSessionServiceBackend extends TerminalSessionServiceControls {
  backend: InMemoryRpcBackend;
}

/** The same fake as a backend of its own, for a spec that needs nothing but the terminal service. */
export function aTerminalSessionServiceBackend(
  scenario: TerminalSessionServiceScenario = {},
): TerminalSessionServiceBackend {
  const { handlers, ...controls } = aTerminalSessionServiceFake(scenario);
  return {
    backend: anInMemoryRpcBackend().implement(TerminalSessionService, handlers),
    ...controls,
  };
}
