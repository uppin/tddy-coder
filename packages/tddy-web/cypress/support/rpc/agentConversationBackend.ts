/**
 * In-memory backend for a conversation with a roster agent — `OpenAgentConversation`,
 * `PromptAgentConversation`, `CancelAgentConversation`
 * (docs/ft/web/session-drawer.md § Add agent).
 *
 * Modelled on `sessionAgentRosterBackend.ts`, and for the same reason: `PromptAgentConversation` is
 * server-streaming, so what the pane must do with a *sequence* of frames cannot be expressed by a
 * single stubbed return value. The fake yields the scenario's chunks one at a time and marks only
 * the last one `last`, which is the daemon's own contract
 * (`packages/tddy-service/proto/connection.proto:496-505`).
 *
 * The handlers are exported separately from the backend for the same reason the roster fake's are:
 * Connect's router fills every method a service implementation omits with an `Unimplemented`
 * handler, so a screen-level backend must spread them into its ONE
 * `.implement(ConnectionService, …)` call rather than registering the service twice.
 */

import { Code, ConnectError, type ServiceImpl } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  AgentConversationChunkSchema,
  CancelAgentConversationResponseSchema,
  ConnectionService,
  OpenAgentConversationResponseSchema,
} from "../../../src/gen/connection_pb";

/** What `OpenAgentConversation` was asked for — the routing half of the call plus who it names. */
export interface OpenedConversation {
  readonly sessionId: string;
  readonly daemonInstanceId: string;
  readonly agentId: string;
  /** The id the caller chose, or "" when it left the daemon to mint one. */
  readonly conversationId: string;
}

/** One `PromptAgentConversation` call, as the pane sent it. */
export interface SentPrompt {
  readonly sessionId: string;
  readonly conversationId: string;
  readonly prompt: string;
}

export interface AgentConversationScenario {
  /**
   * The answer, as the chunks `PromptAgentConversation` yields — one frame each, the last marked
   * `last`. An empty array still yields exactly one (empty) frame, which is what the daemon
   * guarantees so a consumer never has to tell "said nothing" from "nothing arrived".
   */
  answerChunks?: string[];
  /** The `stop_reason` on the final frame. */
  stopReason?: string;
  /** When set, `OpenAgentConversation` fails with this message and no conversation is opened. */
  openFails?: string;
  /** When set, `PromptAgentConversation` fails with this message before yielding any frame. */
  promptFails?: string;
  /** The id the daemon mints when the caller sends an empty `conversation_id`. */
  mintedConversationId?: string;
  /**
   * Hold the answer open: `PromptAgentConversation` subscribes but yields nothing until
   * {@link AgentConversationControls.releaseAnswer} is called. The only way to observe what a
   * surface does *while* a turn is in flight — a fake that answers immediately has no such window.
   */
  holdAnswer?: boolean;
}

export interface AgentConversationControls {
  /** Every `OpenAgentConversation`, in call order. */
  openedConversations: () => OpenedConversation[];
  /** Every `PromptAgentConversation`, in call order. */
  promptsSent: () => SentPrompt[];
  /** Every `conversation_id` passed to `CancelAgentConversation`, in call order. */
  cancelledConversationIds: () => string[];
  /** Let a held answer through. A no-op unless the scenario set `holdAnswer`. */
  releaseAnswer: () => void;
  /**
   * Make the **next** `PromptAgentConversation` fail with `message`, leaving the ones already
   * answered alone. A scenario-level `promptFails` cannot express this: it fails every prompt, so a
   * transcript that lost its earlier exchange would be indistinguishable from one that never had
   * it.
   */
  failNextPrompt: (message: string) => void;
}

export interface AgentConversationFake extends AgentConversationControls {
  handlers: Partial<ServiceImpl<typeof ConnectionService>>;
}

export interface AgentConversationBackend extends AgentConversationControls {
  backend: InMemoryRpcBackend;
}

export function anAgentConversationFake(
  scenario: AgentConversationScenario = {},
): AgentConversationFake {
  const opened: OpenedConversation[] = [];
  const prompts: SentPrompt[] = [];
  const cancelled: string[] = [];
  let nextPromptFailure: string | null = null;
  let letAnswerThrough: (() => void) | null = null;
  const held =
    scenario.holdAnswer === true
      ? new Promise<void>((resolve) => {
          letAnswerThrough = resolve;
        })
      : null;

  const handlers: Partial<ServiceImpl<typeof ConnectionService>> = {
    async openAgentConversation(req) {
      if (scenario.openFails !== undefined) {
        throw new ConnectError(scenario.openFails, Code.FailedPrecondition);
      }
      opened.push({
        sessionId: req.sessionId,
        daemonInstanceId: req.daemonInstanceId,
        agentId: req.agentId,
        conversationId: req.conversationId,
      });
      return create(OpenAgentConversationResponseSchema, {
        // A caller-chosen id is echoed back verbatim; only an empty one is minted for
        // (`connection.proto:479-481`).
        conversationId:
          req.conversationId !== ""
            ? req.conversationId
            : (scenario.mintedConversationId ?? "conversation-minted-1"),
      });
    },
    async *promptAgentConversation(req) {
      prompts.push({
        sessionId: req.sessionId,
        conversationId: req.conversationId,
        prompt: req.prompt,
      });
      if (scenario.promptFails !== undefined) {
        throw new ConnectError(scenario.promptFails, Code.Internal);
      }
      if (nextPromptFailure !== null) {
        const message = nextPromptFailure;
        nextPromptFailure = null;
        throw new ConnectError(message, Code.Internal);
      }
      if (held !== null) await held;
      const chunks = scenario.answerChunks ?? [""];
      const frames = chunks.length === 0 ? [""] : chunks;
      for (let i = 0; i < frames.length; i += 1) {
        const last = i === frames.length - 1;
        yield create(AgentConversationChunkSchema, {
          contentChunk: frames[i],
          stopReason: last ? (scenario.stopReason ?? "EndTurn") : "",
          last,
        });
      }
    },
    async cancelAgentConversation(req) {
      cancelled.push(req.conversationId);
      return create(CancelAgentConversationResponseSchema, {});
    },
  };

  return {
    handlers,
    openedConversations: () => [...opened],
    promptsSent: () => [...prompts],
    cancelledConversationIds: () => [...cancelled],
    releaseAnswer: () => letAnswerThrough?.(),
    failNextPrompt: (message: string) => {
      nextPromptFailure = message;
    },
  };
}

/** The conversation fake on a backend of its own — all a spec mounting the pane alone needs. */
export function anAgentConversationBackend(
  scenario: AgentConversationScenario = {},
): AgentConversationBackend {
  const { handlers, ...controls } = anAgentConversationFake(scenario);
  return {
    backend: anInMemoryRpcBackend().implement(ConnectionService, handlers),
    ...controls,
  };
}
