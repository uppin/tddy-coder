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
  type AcpAgentMessage,
  type AcpClientMessage,
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

/** A JSON-RPC-style error (renders as the chat's workflow-error banner). */
export function acpError(message: string): AcpAgentMessage {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: { case: "error", value: { code: -32603n, message } },
  });
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
 */
export function acpRecordingSession(frames: AcpAgentMessage[] = []) {
  const sent: AcpClientMessage[] = [];
  async function* session(requests: AsyncIterable<AcpClientMessage>) {
    for (const f of frames) {
      yield f;
    }
    for await (const req of requests) {
      const c = req.msg.case;
      if (c === "prompt" || c === "requestPermission") {
        sent.push(req);
      }
    }
  }
  return { session, sent };
}
