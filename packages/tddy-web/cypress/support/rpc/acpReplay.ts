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
export function aReplayBackend(config: {
  counts: number[];
  snapshot?: AcpAgentMessage[];
  details?: Record<string, ToolDetail>;
}) {
  const opens: ReplayOpens = { count: 0, snapshot: 0 };
  const backend = anInMemoryRpcBackend().implement(ConnectionService, {
    async *streamAcpReplay(req: { mode: StreamMode }) {
      if (req.mode === StreamMode.COUNT_THEN_LIVE) {
        opens.count += 1;
        yield* countFrames(config.counts);
      } else {
        opens.snapshot += 1;
        yield* transcriptFrames(config.snapshot ?? []);
      }
      // Keep the stream open so the live-tail consumer stays subscribed.
      await new Promise<void>(() => {});
    },
    getAcpToolCallDetail: (req: { toolCallId: string }) => detailOrNotFound(config.details, req),
  });
  return { backend, opens };
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
