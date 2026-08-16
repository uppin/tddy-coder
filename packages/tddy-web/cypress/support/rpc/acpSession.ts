/**
 * Test helpers for mocking the ACP `AcpService.Session` bidi RPC in component specs — the transport
 * `AgentChat acp` / `useAcpSession` (and thus the pr-stack chat) speaks. Frame builders produce the
 * agent→client `AcpAgentMessage`s using the same tddy conventions the real backend
 * (`tddy-service::convert_acp`) uses, so specs assert the same rendered bubbles as before the switch
 * off `TddyRemote`.
 */

import { create } from "@bufbuild/protobuf";
import {
  AcpAgentMessageSchema,
  StopReason,
  ToolCallStatus,
  type AcpAgentMessage,
  type AcpClientMessage,
  type NewSessionRequest,
} from "../../../src/gen/tddy/acp/v1/acp_pb";

/** A streamed agent message chunk → renders as an "agent" bubble. */
export function acpAgentChunk(text: string): AcpAgentMessage {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: {
      case: "sessionUpdate",
      value: {
        sessionId: { value: "s1" },
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

/** The workflow goal (tddy convention: rides the thought channel) → renders as a "goal" bubble. */
export function acpGoal(text: string): AcpAgentMessage {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: {
      case: "sessionUpdate",
      value: {
        sessionId: { value: "s1" },
        update: {
          update: {
            case: "agentThoughtChunk",
            value: { content: { block: { case: "text", value: { text } } } },
          },
        },
      },
    },
  });
}

/** A `user_message_chunk` (the agent echoing the operator's own prompt). `useAcpSession` ignores it
 *  — the operator's message is already echoed locally by `sendPrompt` — so it renders no bubble. */
export function acpUserMessage(text: string): AcpAgentMessage {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: {
      case: "sessionUpdate",
      value: {
        sessionId: { value: "s1" },
        update: {
          update: {
            case: "userMessageChunk",
            value: { content: { block: { case: "text", value: { text } } } },
          },
        },
      },
    },
  });
}

/** A non-tool activity/system log line (one-shot tool_call) → renders as an "activity" bubble. */
export function acpActivity(text: string): AcpAgentMessage {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: {
      case: "sessionUpdate",
      value: {
        sessionId: { value: "s1" },
        update: {
          update: {
            case: "toolCall",
            value: { toolCallId: { value: "activity" }, title: text },
          },
        },
      },
    },
  });
}

/**
 * A real tool call opening, as `tddy-acp::provider_agent` announces one before dispatching it: an
 * id to correlate its result by, the tool's name as the title, and `in_progress`.
 */
export function acpToolCall(id: string, title: string): AcpAgentMessage {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: {
      case: "sessionUpdate",
      value: {
        sessionId: { value: "s1" },
        update: {
          update: {
            case: "toolCall",
            value: {
              toolCallId: { value: id },
              title,
              status: ToolCallStatus.IN_PROGRESS,
            },
          },
        },
      },
    },
  });
}

/**
 * How a tool call ended: its terminal status and the output it produced, carried as `raw_output`
 * (JSON, the way `tool_output_value` encodes a tool's answer) because the protobuf mirror of ACP
 * carries `raw_output` and not `content`.
 */
export function acpToolCallResult(
  id: string,
  outcome: { failed: boolean; output: string },
): AcpAgentMessage {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: {
      case: "sessionUpdate",
      value: {
        sessionId: { value: "s1" },
        update: {
          update: {
            case: "toolCallUpdate",
            value: {
              toolCallId: { value: id },
              fields: {
                status: outcome.failed ? ToolCallStatus.FAILED : ToolCallStatus.COMPLETED,
                rawOutput: JSON.stringify(outcome.output),
              },
            },
          },
        },
      },
    },
  });
}

/** An agent-initiated permission request for a clarification (mirrors
 *  `convert_acp::clarification_request_permission`). `multi` sets the `:multi` tool-call-id
 *  convention; `allowOther` appends the free-text "other" affordance option; the question text +
 *  header ride the tool-call fields (title = question, raw_input = header). */
export function acpQuestion(
  labels: string[],
  opts: {
    multi?: boolean;
    allowOther?: boolean;
    header?: string;
    question?: string;
    id?: bigint;
  } = {},
): AcpAgentMessage {
  const options = labels.map((name, i) => ({
    optionId: { value: `option-${i}` },
    name,
  }));
  if (opts.allowOther) {
    options.push({ optionId: { value: "other" }, name: "Other…" });
  }
  return create(AcpAgentMessageSchema, {
    id: opts.id ?? 7n,
    msg: {
      case: "requestPermission",
      value: {
        sessionId: { value: "s1" },
        toolCall: {
          toolCallId: { value: opts.multi ? "clarification:multi" : "clarification" },
          fields: { title: opts.question ?? "", rawInput: opts.header ?? "" },
        },
        options,
      },
    },
  });
}

/** A terminal prompt response (turn ended). */
export function acpPromptEnd(id: bigint = 0n): AcpAgentMessage {
  return create(AcpAgentMessageSchema, {
    id,
    msg: { case: "prompt", value: { stopReason: StopReason.END_TURN } },
  });
}

/**
 * A JSON-RPC-style error (renders as the chat's workflow-error banner).
 *
 * `code` defaults to `internal error`; the daemon's model-addressed surface distinguishes seven
 * (`ModelAcpService::AcpErrorCode`), of which `-32001` is a refusal to touch something that is not
 * the caller's.
 */
export function acpError(message: string, code: bigint = -32603n): AcpAgentMessage {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: { case: "error", value: { code, message } },
  });
}

/** `ModelAcpService`'s `PermissionDenied` code — the workspace refusal a chat has to put in words. */
export const ACP_PERMISSION_DENIED = -32001n;

/**
 * The text of every content block a client `prompt` frame carried, in order.
 *
 * Throws for a frame that is not a prompt, or a block that is not text: a spec asserting on what
 * the operator sent must fail on the wrong shape rather than quietly compare against an empty list.
 */
export function promptTexts(m: AcpClientMessage): string[] {
  if (m.msg.case !== "prompt") {
    throw new Error(`expected a prompt frame, got '${String(m.msg.case)}'`);
  }
  return m.msg.value.prompt.map((block) => {
    if (block.block.case !== "text") {
      throw new Error(`expected a text content block, got '${String(block.block.case)}'`);
    }
    return block.block.value.text;
  });
}

/**
 * The `new_session` handshake a client opened the stream with.
 *
 * Throws for a frame that is not one: a spec asserting on which registry row a chat named — and on
 * where its tools were told to run — must fail on the wrong frame rather than compare against a
 * default-constructed request that says neither.
 */
export function newSessionRequest(m: AcpClientMessage): NewSessionRequest {
  if (m.msg.case !== "newSession") {
    throw new Error(`expected a new_session frame, got '${String(m.msg.case)}'`);
  }
  return m.msg.value;
}

/** The encoded `option_id` a client sent in a `requestPermission` reply (`""` if not that shape). */
export function selectedOptionId(m: AcpClientMessage): string {
  if (m.msg.case !== "requestPermission") return "";
  const outcome = m.msg.value.outcome?.outcome;
  return outcome?.case === "selected" ? (outcome.value.optionId?.value ?? "") : "";
}

/** A `session` handler that yields the given frames once, then idles (ignores client input). */
export function acpScriptedSession(...frames: AcpAgentMessage[]) {
  return async function* () {
    for (const f of frames) {
      yield f;
    }
    // Keep the stream open indefinitely so the client's send side stays usable.
    await new Promise<void>(() => {});
  };
}

/**
 * A `session` handler that records the operator's outbound `AcpClientMessage`s (skipping the eager
 * `initialize`/`new_session` handshake) into `sent`, optionally emitting `frames` first. Use for
 * specs asserting what the client sent (prompts, permission replies).
 *
 * The handshake's own `new_session` frames are recorded separately in `opened`: on the daemon's
 * model-addressed surface that frame is where the chat names which registry row it speaks as and
 * where that row's tools may run, so it is a claim in its own right rather than ceremony.
 */
export function acpRecordingSession(frames: AcpAgentMessage[] = []) {
  const sent: AcpClientMessage[] = [];
  const opened: AcpClientMessage[] = [];
  async function* session(requests: AsyncIterable<AcpClientMessage>) {
    for (const f of frames) {
      yield f;
    }
    for await (const req of requests) {
      const c = req.msg.case;
      if (c === "prompt" || c === "requestPermission") {
        sent.push(req);
      }
      if (c === "newSession") {
        opened.push(req);
      }
    }
  }
  return { session, sent, opened };
}
