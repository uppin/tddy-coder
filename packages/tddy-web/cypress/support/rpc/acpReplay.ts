/**
 * Test helpers for the read-only **ACP replay** stream (`ConnectionService.StreamAcpReplay`) that
 * backs the Agent Activity overlay's transcript. The server-streaming RPC emits ACP-format
 * `AcpAgentMessage` frames (only the `session_update` variant), each carrying a wall-clock
 * `timestamp_unix_ms` on the `SessionNotification` wrapper so the transcript can render its
 * DEBUG-style "+Ns" elapsed badge. Frame builders mirror what `tddy-service::acp_replay` produces
 * from `conversation.jsonl` (+ `agent-activity.jsonl`), so specs assert the rendered transcript.
 */

import { create, toBinary } from "@bufbuild/protobuf";
import { ConnectError, Code } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ConnectionService,
  AcpReplayFrameSchema,
  GetAcpToolCallDetailResponseSchema,
  StreamMode,
} from "../../../src/gen/connection_pb";
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

/** A replayed tool call stamped at `atUnixMs`. `rawInput` carries the full tool input as JSON — the
 *  transcript derives the inline detail (e.g. `Read main.rs L10-49`) from it. `output`, when given,
 *  rides as `rawOutput` (the tool's result) so the detail dialog can render it. */
export function replayToolCall(fields: {
  id: string;
  title: string;
  kind: ToolKind;
  status: ToolCallStatus;
  input: unknown;
  output?: unknown;
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
              rawInput: JSON.stringify(fields.input),
              ...(fields.output === undefined
                ? {}
                : { rawOutput: JSON.stringify(fields.output) }),
            },
          },
        },
      },
    },
  });
}

/** A **body-less** replayed tool call stamped at `atUnixMs` — mirrors the server-stripped stream
 *  (PR #345): it carries only `tool_call_id`/`title`/`kind`/`status`, no `raw_input`/`raw_output`.
 *  The bodies are fetched on demand via `GetAcpToolCallDetail` (see the `details` config on
 *  {@link aReplayBackend}). */
export function replayToolCallStripped(fields: {
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
              // No rawInput / rawOutput — the stream no longer inlines bodies.
            },
          },
        },
      },
    },
  });
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
  /** Bodies served by the on-demand `GetAcpToolCallDetail` lookup, keyed by `tool_call_id`. A
   *  `tool_call_id` absent from this map is answered `NOT_FOUND` — exactly as the real server
   *  answers for an unknown call — which drives the detail dialog's error path. */
  details?: Record<string, { rawInput?: string; rawOutput?: string }>;
  /** When true, the `GetAcpToolCallDetail` response is withheld until {@link ReplayBackendHandle.releaseDetail}
   *  is called, so a spec can observe the dialog's loading state while the fetch is in flight. */
  holdDetail?: boolean;
}): ReplayBackendHandle {
  const opens: ReplayOpens = { count: 0, snapshot: 0 };
  let releaseDetail: () => void = () => undefined;
  const detailGate = new Promise<void>((resolve) => {
    releaseDetail = resolve;
  });
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
    async getAcpToolCallDetail(req: { toolCallId: string }) {
      if (config.holdDetail) await detailGate;
      const detail = config.details?.[req.toolCallId];
      if (!detail) {
        throw new ConnectError(
          `no tool call with id ${req.toolCallId}`,
          Code.NotFound,
        );
      }
      return create(GetAcpToolCallDetailResponseSchema, {
        rawInput: detail.rawInput,
        rawOutput: detail.rawOutput,
      });
    },
  });
  return { backend, opens, releaseDetail: () => releaseDetail() };
}

/** What {@link aReplayBackend} returns: the in-memory backend, the stream-open tallies, and a
 *  release for a held detail response (a no-op unless `holdDetail` was set). */
export interface ReplayBackendHandle {
  backend: InMemoryRpcBackend;
  opens: ReplayOpens;
  releaseDetail: () => void;
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

export { StreamMode, ToolCallStatus, ToolKind };
