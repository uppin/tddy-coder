/**
 * What the Models & Agents chat is opened against: one row of some daemon's model registry.
 *
 * The screen chats with two different things through the *same* ACP stream — a bare model from the
 * catalog, and an assistant composed from one — and the daemon resolves them differently
 * (`ModelAcpService::resolve_target`: an `assistant_id` brings its own provider, system prompt and
 * tools; a `provider_id` + `model_id` brings none of those). This type is the one place that
 * difference is decided, so the dialog itself needs to know nothing about which kind it is showing.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC10, § ACP chat).
 */

import type { AssistantRow, ModelRow } from "./mergeRegistryEntries";

export interface RegistryChatTarget {
  /** The daemon that owns the row — the one that serves this chat, never the selected one. */
  readonly daemonInstanceId: string;
  /** The heading: the model's or the assistant's display label. */
  readonly label: string;
  /** The model this chat speaks as, shown alongside the heading. */
  readonly modelId: string;
  /** The provider the model is offered by; empty for an assistant, which carries its own. */
  readonly providerId: string;
  /** Non-empty when the chat is with an assistant rather than a bare model. */
  readonly assistantId: string;
  /**
   * The workspace this chat's tools run in, on the owning daemon. Empty for a chat that runs no
   * tools; the daemon refuses an empty workspace for a tool-bearing assistant, so it is never left
   * empty in the hope that one is not needed.
   */
  readonly cwd: string;
}

/**
 * A chat with a model straight out of the catalog: no system prompt, no tools, and therefore no
 * workspace to run them in.
 */
export function chatWithModel(model: ModelRow): RegistryChatTarget {
  return {
    daemonInstanceId: model.daemonInstanceId,
    label: model.label,
    modelId: model.modelId,
    providerId: model.providerId,
    assistantId: "",
    cwd: "",
  };
}

/**
 * A chat with an assistant, whose tools run in `cwd`.
 *
 * Only the assistant's id is sent as the target: its provider, model, system prompt and tool set
 * are the daemon's own record, and re-stating them from the browser would let a stale table decide
 * what an assistant is.
 */
export function chatWithAssistant(assistant: AssistantRow, cwd: string): RegistryChatTarget {
  return {
    daemonInstanceId: assistant.daemonInstanceId,
    label: assistant.label,
    modelId: assistant.modelId,
    providerId: "",
    assistantId: assistant.assistantId,
    cwd,
  };
}

/**
 * Whether an assistant needs a workspace named before it can be chatted with.
 *
 * The daemon confines a tool-bearing assistant's tools to a directory the caller already owns and
 * refuses an empty `cwd` outright (`resolve_chat_workspace`), so the choice is made before the
 * stream opens rather than discovered as a failed handshake. An assistant with no tools reaches no
 * engine at all and needs none.
 */
export function needsWorkspace(assistant: AssistantRow): boolean {
  return assistant.tools.length > 0;
}
