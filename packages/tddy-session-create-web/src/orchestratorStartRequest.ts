import type { SessionAttachmentInit, StartSessionRequestInit } from "./hooks/useSessionAttachments";
import type { SessionType } from "./createSessionRequest";
import type { SelectableAgent } from "./selectableAgentOptions";

/**
 * Everything the **orchestrator composer** (`#orchestrators` 8/11) sends: the project its "+" was
 * pressed on, the text typed in its box, and the five controls under the box — model, effort,
 * provider account, host and attachments. The session type is not a control: it is the first agent
 * the chosen host serves whose spec declares `project_home_main` ({@link orchestratorSessionTypeFor}).
 */
export interface OrchestratorComposerValues {
  sessionToken: string;
  projectId: string;
  /** The host the orchestrator runs on — the composer's host picker. */
  daemonInstanceId: string;
  sessionType: SessionType;
  /** The text in the composer's box: the orchestrator's first prompt. */
  initialPrompt: string;
  model: string;
  /** One of the agent's served effort levels; empty for the agent's own default. */
  effort: string;
  /** The provider account the agent runs on; empty for its vendor. */
  providerAccountId: string;
  /** The operator explicitly chose the vendor, overriding any default account. */
  providerBuiltIn: boolean;
  /** The documents the daemon materializes before the agent starts, already staged. */
  attachments: SessionAttachmentInit[];
}

/**
 * The `StartSession` request the composer sends: `session_role: orchestrator` and only what the
 * operator chose. No worktree, branch, placement or stack field — the daemon places an orchestrator
 * on the project's home worktree itself (`#orchestrators` 1/11).
 *
 * Not `createSessionRequest.ts`'s `buildOrchestratorStartRequest` (the New-session form's switch,
 * which sends no attachments): the composer's request is its own, built from its own five controls.
 */
export function buildOrchestratorComposerRequest(
  values: OrchestratorComposerValues,
): StartSessionRequestInit {
  // TODO(orchestrator-composer): implement
  void values;
  return {};
}

/**
 * The session type an orchestrator on `daemonInstanceId` starts as: the first agent that host serves
 * whose spec declares `project_home_main` — the agents that can be held read-only, as main's setup
 * form picks. `null` when the host serves none.
 */
export function orchestratorSessionTypeFor(
  agents: readonly SelectableAgent[],
  daemonInstanceId: string,
): SessionType | null {
  // TODO(orchestrator-composer): implement
  void agents;
  void daemonInstanceId;
  return null;
}
