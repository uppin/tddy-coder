/**
 * Test helpers for the read-only **ACP replay** stream (`ConnectionService.StreamAcpReplay`) that
 * backs the Agent Activity overlay's transcript. The server-streaming RPC emits ACP-format
 * `AcpAgentMessage` frames (only the `session_update` variant), each carrying a wall-clock
 * `timestamp_unix_ms` on the `SessionNotification` wrapper so the transcript can render its
 * DEBUG-style "+Ns" elapsed badge. Frame builders mirror what `tddy-service::acp_replay` produces
 * from `conversation.jsonl` (+ `agent-activity.jsonl`), so specs assert the rendered transcript.
 */

import { create, toBinary } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ConnectionService, AcpReplayFrameSchema } from "../../../src/gen/connection_pb";
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
 *  transcript derives the inline detail (e.g. `Read main.rs L10-49`) from it. */
export function replayToolCall(fields: {
  id: string;
  title: string;
  kind: ToolKind;
  status: ToolCallStatus;
  input: unknown;
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
            },
          },
        },
      },
    },
  });
}

/** A backend whose `StreamAcpReplay` yields exactly `frames` (each wrapped in an `AcpReplayFrame`),
 *  then stays open. */
export function backendReplaying(...frames: AcpAgentMessage[]) {
  return anInMemoryRpcBackend().implement(ConnectionService, {
    async *streamAcpReplay() {
      for (const frame of frames) {
        // The frame rides as protobuf bytes; the client decodes them with the AcpAgentMessage schema.
        yield create(AcpReplayFrameSchema, {
          acpAgentMessage: toBinary(AcpAgentMessageSchema, frame),
        });
      }
      // Keep the stream open so the live-tail consumer stays subscribed.
      await new Promise<void>(() => {});
    },
  });
}

export { ToolCallStatus, ToolKind };
