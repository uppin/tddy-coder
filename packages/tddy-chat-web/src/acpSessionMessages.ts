/**
 * The ACP frames `useAcpSession` sends and the question it reads out of a `request_permission` —
 * pure builders over the generated `AcpService` messages, split from the hook so its body is the
 * stream lifecycle alone.
 */
import { create } from "@bufbuild/protobuf";
import {
  AcpClientMessageSchema,
  type AcpClientMessage,
  type ModelSessionTarget,
  type RequestPermissionRequest,
} from "tddy-client-web/gen/tddy/acp/v1/acp_pb";
import type { ElicitationPoint, PendingQuestion } from "./useAgentChat";

/**
 * What a chat with a row of the daemon's model registry has to name in its handshake.
 *
 * Both halves are the session's own property rather than the stream's: one stream may open a
 * session for a different assistant next time, and each assistant runs its tools somewhere else.
 */
export interface RegistryChatSession {
  /** Which registry row this session speaks as — a model, or an assistant. */
  readonly target: ModelSessionTarget;
  /**
   * The workspace an assistant's tools run in, on the daemon serving the chat. `""` for a chat that
   * runs no tools; the daemon refuses an empty workspace for a tool-bearing assistant rather than
   * choosing one on the operator's behalf.
   */
  readonly cwd: string;
}

/**
 * The stream-opening handshake: `initialize` (id 1) then either `session/load` (resume, id 2) or
 * `new_session` (id 2, carrying the registry row when there is one).
 */
export function acpHandshake(
  resumeSessionId: string | undefined,
  registry: RegistryChatSession | undefined,
): AcpClientMessage[] {
  return [
    create(AcpClientMessageSchema, { id: 1n, msg: { case: "initialize", value: {} } }),
    resumeSessionId
      ? create(AcpClientMessageSchema, {
          id: 2n,
          msg: { case: "loadSession", value: { sessionId: { value: resumeSessionId }, cwd: "" } },
        })
      : create(AcpClientMessageSchema, {
          id: 2n,
          msg: {
            case: "newSession",
            value: { cwd: registry?.cwd ?? "", modelTarget: registry?.target },
          },
        }),
  ];
}

/** A `PromptRequest` carrying `text` into the session `sessionId`. */
export function acpPrompt(id: bigint, sessionId: string, text: string): AcpClientMessage {
  return create(AcpClientMessageSchema, {
    id,
    msg: {
      case: "prompt",
      value: {
        sessionId: { value: sessionId },
        prompt: [{ block: { case: "text", value: { text } } }],
      },
    },
  });
}

/** A `request_permission` reply selecting the option whose id encodes the answer. */
export function acpPermissionReply(id: bigint, optionId: string): AcpClientMessage {
  return create(AcpClientMessageSchema, {
    id,
    msg: {
      case: "requestPermission",
      value: {
        outcome: {
          outcome: { case: "selected", value: { optionId: { value: optionId } } },
        },
      },
    },
  });
}

/**
 * The clarification a `request_permission` asks, by tddy convention: the `:multi` tool-call-id
 * suffix marks a multi-select; the question text and header ride the tool-call fields (title =
 * question, raw_input = header); the `other` option is the free-text affordance.
 */
export function acpQuestionOf(
  request: RequestPermissionRequest,
  at: number,
): { pending: PendingQuestion; elicitation: ElicitationPoint } {
  const toolCall = request.toolCall;
  const kind = toolCall?.toolCallId?.value?.endsWith(":multi") ? "multiSelect" : "select";
  const options = request.options
    .filter((o) => o.optionId?.value !== "other")
    .map((o) => ({ label: o.name, description: "" }));
  const allowOther = request.options.some((o) => o.optionId?.value === "other");
  const header = toolCall?.fields?.rawInput ?? "";
  const question = toolCall?.fields?.title ?? "";
  return {
    pending: { kind, header, question, options, allowOther },
    elicitation: { at, kind, header, question, options: options.map((o) => o.label), allowOther },
  };
}
