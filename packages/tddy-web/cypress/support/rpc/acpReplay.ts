/**
 * Test helpers for the read-only **ACP replay** stream (`ConnectionService.StreamAcpReplay`) that
 * backs the Agent Activity overlay's transcript. The server-streaming RPC emits ACP-format
 * `AcpAgentMessage` frames (only the `session_update` variant), each carrying a wall-clock
 * `timestamp_unix_ms` on the `SessionNotification` wrapper so the transcript can render its
 * DEBUG-style "+Ns" elapsed badge. Frame builders mirror what `tddy-service::acp_replay` produces
 * from `conversation.jsonl` (+ `agent-activity.jsonl`), so specs assert the rendered transcript.
 */

import { create, toBinary } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ConnectionService, AcpReplayFrameSchema, StreamMode } from "../../../src/gen/connection_pb";
import {
  AcpAgentMessageSchema,
  ToolCallStatus,
  ToolKind,
  type AcpAgentMessage,
} from "../../../src/gen/tddy/acp/v1/acp_pb";

/** A replayed agent text chunk stamped at `atUnixMs` → an "agent" transcript bubble. */
export function replayAgentText(text: string, atUnixMs: number): AcpAgentMessage {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: {
      case: "sessionUpdate",
      value: {
        sessionId: { value: "s1" },
        timestampUnixMs: BigInt(atUnixMs),
        update: {
          update: {
            case: "agentMessageChunk",
            value: { content: { block: { case: "text", value: { text } } } },
          },
        },
      },
    },
  });
}

/**
 * A replayed tool call stamped at `atUnixMs`, carrying **metadata only** — `title`, `kind`, `status`,
 * `tool_call_id`. The real hosts strip `raw_input`/`raw_output` out of every streamed frame
 * (`tddy_service::acp_replay::strip_tool_body`), so a frame builder that inlined them would model a
 * server that no longer exists. Bodies are served separately by `GetAcpToolCallDetail` — see
 * {@link aToolDetail} and the `details` map on {@link aReplayBackend}.
 */
export function replayToolCall(fields: {
  id: string;
  title: string;
  kind: ToolKind;
  status: ToolCallStatus;
  atUnixMs: number;
}): AcpAgentMessage {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: {
      case: "sessionUpdate",
      value: {
        sessionId: { value: "s1" },
        timestampUnixMs: BigInt(fields.atUnixMs),
        update: {
          update: {
            case: "toolCall",
            value: {
              toolCallId: { value: fields.id },
              title: fields.title,
              kind: fields.kind,
              status: fields.status,
            },
          },
        },
      },
    },
  });
}

/** One tool call's bodies as `GetAcpToolCallDetail` returns them: the exact JSON strings the stream
 *  used to inline. `output` is omitted for a call that has not produced one yet — the response field
 *  is `optional`, and an absent output is a success, not an error. */
export function aToolDetail(fields: { input: unknown; output?: unknown }): ToolDetail {
  return {
    rawInput: JSON.stringify(fields.input),
    ...(fields.output === undefined ? {} : { rawOutput: JSON.stringify(fields.output) }),
  };
}

/** One tool call's bodies as the lookup returns them when it has an **output but no input**. Both
 *  response fields are `optional`, so a resolved detail may legitimately carry neither, either, or
 *  both; this models the asymmetric case the `input`-taking {@link aToolDetail} cannot express. */
export function aToolDetailWithoutInput(fields: { output: unknown }): ToolDetail {
  return { rawOutput: JSON.stringify(fields.output) };
}

/** The bodies of one tool call, keyed by `tool_call_id` in a backend's `details` map. Both sides are
 *  optional, mirroring `GetAcpToolCallDetailResponse`. */
export interface ToolDetail {
  rawInput?: string;
  rawOutput?: string;
}

/** A backend whose `StreamAcpReplay` serves the two-phase protocol for a fixed transcript: the
 *  `COUNT_THEN_LIVE` feed reports one count of `frames.length`, and the `SNAPSHOT_THEN_LIVE` feed
 *  yields the `frames` transcript. Both stay open. Delegates to {@link aReplayBackend} so the mode
 *  branching lives in one place. */
export function backendReplaying(...frames: AcpAgentMessage[]) {
  return aReplayBackend({ counts: [frames.length], snapshot: frames }).backend;
}

/** Records how many times each stream mode was opened on the backend, so a spec can assert lazy /
 *  once-only subscription behaviour (the in-memory testkit records unary calls only, not streams). */
export interface ReplayOpens {
  /** COUNT_THEN_LIVE subscriptions opened (the cheap icon/badge feed). */
  count: number;
  /** SNAPSHOT_THEN_LIVE subscriptions opened (the lazy full-transcript pull). */
  snapshot: number;
}

/**
 * A `StreamAcpReplay` backend that answers the two phases separately:
 *
 * - `COUNT_THEN_LIVE` → yields one `activity_count` frame per entry in `counts` (e.g. `[3]` for a
 *   fixed count, or `[1, 2]` to model a live increment), then stays open. No transcript payload.
 * - `SNAPSHOT_THEN_LIVE` → yields the `snapshot` transcript frames, then stays open.
 *
 * `opens` tallies how many times each mode was subscribed, so specs can assert the count feed drives
 * the icon without pulling the snapshot, and that the snapshot is pulled lazily and only once.
 */
export function aReplayBackend(config: AcpReplayScenario) {
  const opens: ReplayOpens = { count: 0, snapshot: 0 };
  const backend = anInMemoryRpcBackend().implement(
    ConnectionService,
    acpReplayHandlers(config, opens),
  );
  return { backend, opens };
}

/** A fixed transcript to replay: the counts the cheap feed reports, the frames the snapshot feed
 *  yields, and the tool-call bodies `GetAcpToolCallDetail` resolves. */
export interface AcpReplayScenario {
  counts: number[];
  snapshot?: AcpAgentMessage[];
  details?: Record<string, ToolDetail>;
  /** When set, the COUNT feed subscribes but stays silent until this resolves — see
   *  {@link aHeldCountReplay}. */
  holdCount?: Promise<void>;
}

/**
 * A replay scenario whose **count** feed is subscribed but silent until `releaseCount()` is called.
 *
 * Models the window every session opens in: the stream is live but has not answered yet, so the app
 * knows neither "this session recorded nothing" nor "it recorded something". A surface that renders
 * that difference must be provably quiet in this window — and since a failed feed never answers
 * either, the same window stands in for a failed read. Mirrors
 * {@link aReplayBackendWithHeldSnapshot}, one phase earlier.
 */
export function aHeldCountReplay(config: AcpReplayScenario) {
  let release: () => void = () => undefined;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  return {
    scenario: { ...config, holdCount: held } satisfies AcpReplayScenario,
    releaseCount: () => release(),
  };
}

/**
 * The `StreamAcpReplay` + `GetAcpToolCallDetail` handlers for a fixed transcript, as a spreadable
 * `ConnectionService` partial. Extracted so the two-phase protocol has ONE implementation shared by
 * the focused {@link aReplayBackend} and the full-screen `aConnectionServiceBackend` — a spec that
 * drives `SessionsDrawerScreen` needs both the session list and the replay on one backend, and a
 * second copy of the mode branching would be free to drift from this one.
 *
 * `opens` is tallied in place when supplied, so a caller can assert lazy / once-only subscription.
 */
export function acpReplayHandlers(config: AcpReplayScenario, opens?: ReplayOpens) {
  return {
    async *streamAcpReplay(req: { mode: StreamMode }) {
      if (req.mode === StreamMode.COUNT_THEN_LIVE) {
        if (opens) opens.count += 1;
        if (config.holdCount) await config.holdCount;
        yield* countFrames(config.counts);
      } else {
        if (opens) opens.snapshot += 1;
        yield* transcriptFrames(config.snapshot ?? []);
      }
      // Keep the stream open so the live-tail consumer stays subscribed.
      await new Promise<void>(() => {});
    },
    getAcpToolCallDetail: (req: { toolCallId: string }) => detailOrNotFound(config.details, req),
  };
}

/**
 * A backend whose **`GetAcpToolCallDetail`** unary is held open — received, but silent — until the test
 * calls `releaseDetail()`. The replay stream answers immediately, exactly as in {@link aReplayBackend}.
 *
 * Lets a spec observe the dialog while the body lookup is genuinely **in flight**, which is what
 * production does: the real lookup crosses a network and re-reads the session transcript from disk.
 */
export function aReplayBackendWithHeldDetail(config: {
  counts: number[];
  snapshot: AcpAgentMessage[];
  details: Record<string, ToolDetail>;
}) {
  const opens: ReplayOpens = { count: 0, snapshot: 0 };
  let release: () => void = () => undefined;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const backend = anInMemoryRpcBackend().implement(ConnectionService, {
    async *streamAcpReplay(req: { mode: StreamMode }) {
      if (req.mode === StreamMode.COUNT_THEN_LIVE) {
        opens.count += 1;
        yield* countFrames(config.counts);
      } else {
        opens.snapshot += 1;
        yield* transcriptFrames(config.snapshot);
      }
      await new Promise<void>(() => {});
    },
    getAcpToolCallDetail: async (req: { toolCallId: string }) => {
      await held;
      return detailOrNotFound(config.details, req);
    },
  });
  return { backend, opens, releaseDetail: () => release() };
}

/**
 * A backend whose `GetAcpToolCallDetail` always fails with `code` — a **transport-level** failure
 * (default `UNAVAILABLE`), as distinct from the `NOT_FOUND` that {@link aReplayBackend} raises for a
 * `tool_call_id` absent from the transcript. The replay stream answers normally.
 */
export function aReplayBackendWithFailingDetail(config: {
  counts: number[];
  snapshot: AcpAgentMessage[];
  code?: Code;
}) {
  const opens: ReplayOpens = { count: 0, snapshot: 0 };
  const backend = anInMemoryRpcBackend().implement(ConnectionService, {
    async *streamAcpReplay(req: { mode: StreamMode }) {
      if (req.mode === StreamMode.COUNT_THEN_LIVE) {
        opens.count += 1;
        yield* countFrames(config.counts);
      } else {
        opens.snapshot += 1;
        yield* transcriptFrames(config.snapshot);
      }
      await new Promise<void>(() => {});
    },
    getAcpToolCallDetail: () => {
      throw new ConnectError("replay host unreachable", config.code ?? Code.Unavailable);
    },
  });
  return { backend, opens };
}

/** The `tool_call_id`s the component has asked bodies for, in call order. Lets a spec assert both
 *  *which* call was looked up and *how many* lookups happened (the cache's observable contract)
 *  without reaching into the RPC plumbing. */
export function requestedToolCallIds(backend: InMemoryRpcBackend): string[] {
  return backend
    .callsTo(ConnectionService.method.getAcpToolCallDetail)
    .map((req) => req.toolCallId);
}

/** Resolve one call's bodies from a `details` map. An id the map does not carry is a `NOT_FOUND`
 *  error — mirroring the hosts, which map a `tool_call_id` absent from the transcript to `NOT_FOUND`
 *  rather than to an empty success. */
function detailOrNotFound(
  details: Record<string, ToolDetail> | undefined,
  req: { toolCallId: string },
): ToolDetail {
  const detail = (details ?? {})[req.toolCallId];
  if (!detail) {
    throw new ConnectError(`no tool call ${req.toolCallId} in transcript`, Code.NotFound);
  }
  return detail;
}

/**
 * A `StreamAcpReplay` backend whose **snapshot** feed is held open — subscribed, but silent — until
 * the test calls `releaseSnapshot()`. The count feed answers immediately, exactly as in
 * {@link aReplayBackend}.
 *
 * Lets a spec interleave host activity (a re-render, a remount) with an **in-flight** snapshot pull,
 * which is what production does: the real snapshot crosses a network and the dashboard re-renders
 * while it is in flight. `opens.snapshot` still tallies subscriptions, so a spec can synchronize on
 * "the pull has started" before acting.
 */
export function aReplayBackendWithHeldSnapshot(config: {
  counts: number[];
  snapshot: AcpAgentMessage[];
}) {
  const opens: ReplayOpens = { count: 0, snapshot: 0 };
  let release: () => void = () => undefined;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const backend = anInMemoryRpcBackend().implement(ConnectionService, {
    async *streamAcpReplay(req: { mode: StreamMode }) {
      if (req.mode === StreamMode.COUNT_THEN_LIVE) {
        opens.count += 1;
        yield* countFrames(config.counts);
      } else {
        opens.snapshot += 1;
        await held;
        yield* transcriptFrames(config.snapshot);
      }
      // Keep the stream open so the live-tail consumer stays subscribed.
      await new Promise<void>(() => {});
    },
  });
  return { backend, opens, releaseSnapshot: () => release() };
}

// ---------------------------------------------------------------------------
// Tail-first replay: paged transcript + live tail
// ---------------------------------------------------------------------------

/**
 * The wire value of `StreamMode.TAIL_THEN_LIVE` — replay only the newest `page_size` persisted
 * frames, oldest-first within the page, then tail live. Re-exported as a plain number because the
 * fake compares it against `Number(req.mode)` and specs assert it inside a recorded request.
 */
export const TAIL_THEN_LIVE: number = StreamMode.TAIL_THEN_LIVE;

/** The server's default page size when a request leaves `page_size` at 0 — mirrors
 *  `tddy_service::acp_replay::DEFAULT_REPLAY_PAGE_SIZE`. */
export const DEFAULT_REPLAY_PAGE_SIZE = 100;

/**
 * A recorded transcript of `entryCount` agent-text entries, one second apart, labelled `Entry 1` …
 * `Entry N` (**1-based**, so the label names the entry's position in the whole transcript rather than
 * its index within whichever page happens to be loaded). A spec asserting "the newest page opens at
 * Entry 151" therefore states the position it means.
 */
export function aRecordedTranscript(entryCount: number): AcpAgentMessage[] {
  return Array.from({ length: entryCount }, (_, i) =>
    replayAgentText(`Entry ${i + 1}`, 1_000 + i * 1_000),
  );
}

/** One subscription to the transcript feed, as the client opened it. */
export interface TranscriptOpen {
  /** `StreamMode` wire value — {@link TAIL_THEN_LIVE} for a paged open, 0 for the whole-history one. */
  readonly mode: number;
  /** Requested `page_size`; 0 means the client asked for the server default. */
  readonly pageSize: number;
}

export interface TailReplayScenario {
  /** The session's whole recorded transcript, oldest-first — what the daemon holds on disk. */
  transcript: AcpAgentMessage[];
  /** Frames the server serves per page. Defaults to {@link DEFAULT_REPLAY_PAGE_SIZE}. */
  serverPageSize?: number;
  /** When set, every `GetAcpReplayPage` fails with this code instead of serving a page — the cursor
   *  is still recorded, so a spec can prove a failed fetch is retried. */
  failPagesWith?: Code;
  /** When set, `GetAcpReplayPage` is received but stays silent until this resolves — see
   *  {@link aTailReplayWithHeldPages}. */
  holdPages?: Promise<void>;
  details?: Record<string, ToolDetail>;
}

/**
 * A tail-replay scenario whose **older-page** fetch is received but unanswered until
 * `releasePage()` is called.
 *
 * The in-flight window is the only state in which the paging indicator exists, and a fake that
 * answers instantly closes it before a spec can look. Without this, `agent-chat-older-loading` can
 * only ever be asserted absent — which a component that never renders it would satisfy too.
 * Mirrors {@link aReplayBackendWithHeldSnapshot}, one surface over.
 */
export function aTailReplayWithHeldPages(config: TailReplayScenario) {
  let release: () => void = () => undefined;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  return {
    replay: aTailReplayBackend({ ...config, holdPages: held }),
    releasePage: () => release(),
  };
}

export interface TailReplayBackend {
  backend: InMemoryRpcBackend;
  /** Every transcript-feed subscription, in order. Streams are not recorded by the testkit, so the
   *  fake tallies its own — this is how a spec pins *which mode and page size* the client opened. */
  transcriptOpens: () => TranscriptOpen[];
  /** The `before_seq` of every `GetAcpReplayPage` the client issued, in order — including calls the
   *  scenario then failed. */
  pageCursors: () => number[];
  /** Append one frame to the live tail of every open transcript stream, stamped with the position it
   *  would have on a later re-read (continuing from the recorded transcript's length). */
  pushLive: (frame: AcpAgentMessage) => void;
}

/**
 * A `StreamAcpReplay` backend that serves the **paged** transcript protocol over a fixed recorded
 * transcript, modelling the daemon faithfully in both directions:
 *
 * - `COUNT_THEN_LIVE` → one `activity_count` frame of `transcript.length`, then stays open.
 * - {@link TAIL_THEN_LIVE} → the newest `page_size` frames, oldest-first within the page, each
 *   stamped with its absolute `seq`, then tails whatever {@link TailReplayBackend.pushLive} appends.
 * - any other mode (i.e. `SNAPSHOT_THEN_LIVE`) → the **whole** transcript head-first, as the server
 *   does today. Modelling this rather than serving a tail page for every mode is what makes a spec
 *   fail when the client opens the wrong mode, instead of passing on the fake's generosity.
 *
 * `GetAcpReplayPage` answers the reverse cursor: frames strictly older than `before_seq`, oldest
 * first, reporting `at_oldest` once the page reaches the transcript head.
 */
export function aTailReplayBackend(config: TailReplayScenario): TailReplayBackend {
  const serverPageSize = config.serverPageSize ?? DEFAULT_REPLAY_PAGE_SIZE;
  const opens: TranscriptOpen[] = [];
  const cursors: number[] = [];
  const live = aLiveTail();

  const handlers = {
    async *streamAcpReplay(req: { mode: StreamMode; pageSize?: number }) {
      if (req.mode === StreamMode.COUNT_THEN_LIVE) {
        yield* countFrames([config.transcript.length]);
        // Keep the count feed open, as the daemon does.
        await new Promise<void>(() => {});
        return;
      }

      const requestedSize = Number(req.pageSize ?? 0);
      opens.push({ mode: Number(req.mode), pageSize: requestedSize });

      const size = requestedSize || serverPageSize;
      const firstSeq =
        Number(req.mode) === TAIL_THEN_LIVE ? Math.max(0, config.transcript.length - size) : 0;
      yield* seqStampedFrames(config.transcript.slice(firstSeq), firstSeq);
      yield* live.framesFrom(config.transcript.length);
    },

    async getAcpReplayPage(req: { beforeSeq: bigint; pageSize: number }) {
      const beforeSeq = Number(req.beforeSeq);
      cursors.push(beforeSeq);
      if (config.holdPages) await config.holdPages;
      if (config.failPagesWith) {
        throw new ConnectError("replay host unreachable", config.failPagesWith);
      }
      const size = Number(req.pageSize) || serverPageSize;
      const firstSeq = Math.max(0, beforeSeq - size);
      const frames = config.transcript.slice(firstSeq, beforeSeq);
      return {
        frames: frames.map((frame) => toBinary(AcpAgentMessageSchema, frame)),
        firstSeq: BigInt(firstSeq),
        atOldest: firstSeq === 0,
      };
    },

    getAcpToolCallDetail: (req: { toolCallId: string }) => detailOrNotFound(config.details, req),
  };

  const backend = anInMemoryRpcBackend().implement(ConnectionService, handlers);

  return {
    backend,
    transcriptOpens: () => opens.slice(),
    pageCursors: () => cursors.slice(),
    pushLive: (frame) => live.push(frame),
  };
}

/**
 * The shared live tail behind every open transcript stream. Frames are held in one list and each
 * subscriber walks it at its own cursor, so a remount (two concurrent streams) sees the same tail
 * rather than splitting it — and the generator never returns, keeping the stream open.
 */
function aLiveTail() {
  const pushed: AcpAgentMessage[] = [];
  const wakers = new Set<() => void>();
  return {
    push(frame: AcpAgentMessage) {
      pushed.push(frame);
      const waiting = [...wakers];
      wakers.clear();
      for (const wake of waiting) wake();
    },
    async *framesFrom(firstSeq: number) {
      let cursor = 0;
      for (;;) {
        while (cursor < pushed.length) {
          yield* seqStampedFrames([pushed[cursor]], firstSeq + cursor);
          cursor += 1;
        }
        await new Promise<void>((resolve) => wakers.add(resolve));
      }
    },
  };
}

/** Transcript frames stamped with their absolute 0-based position, `frames[0]` sitting at `firstSeq`. */
function* seqStampedFrames(frames: AcpAgentMessage[], firstSeq: number) {
  for (const [offset, frame] of frames.entries()) {
    yield create(AcpReplayFrameSchema, {
      acpAgentMessage: toBinary(AcpAgentMessageSchema, frame),
      seq: BigInt(firstSeq + offset),
    });
  }
}

/** Count-only frames (no transcript payload) — the cheap icon/badge feed. */
function* countFrames(counts: number[]) {
  for (const count of counts) {
    yield create(AcpReplayFrameSchema, {
      acpAgentMessage: new Uint8Array(),
      activityCount: BigInt(count),
    });
  }
}

/** Transcript frames (no count) — the heavy snapshot feed. */
function* transcriptFrames(frames: AcpAgentMessage[]) {
  for (const frame of frames) {
    yield create(AcpReplayFrameSchema, {
      acpAgentMessage: toBinary(AcpAgentMessageSchema, frame),
    });
  }
}

export { Code, StreamMode, ToolCallStatus, ToolKind };
