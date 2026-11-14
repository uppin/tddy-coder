import type React from "react";

export interface OrchestratorComposerProps {
  /** The project whose header's "+" opened the composer: the orchestrator's project. */
  projectId: string;
  /** The host the composer opens on — the drawer's selected host; its host picker can change it. */
  daemonInstanceId: string;
  sessionToken: string;
  /** The started orchestrator's id: the screen selects and attaches it as a created session. */
  onStarted: (sessionId: string) => void;
}

/**
 * The **orchestrator composer** (`#orchestrators` 8/11), opened by the "+" on a project header at
 * `/sessions/project/:projectId/new`: a heading and a prompt box centred vertically in the detail
 * pane, and under the box only these controls — **model**, **effort**, **provider account**,
 * **host** and **attachments**, the New-session form's own pieces. Send (Enter, or the send button)
 * starts an orchestrator for the project (`session_role: orchestrator`, placed by the daemon on the
 * project's home worktree) with the text as its first prompt; a refusal is shown under the box with
 * the daemon's reason and the text is kept.
 *
 * Its own component — it neither renders nor extends `CreateSessionPane` (over budget, see
 * `docs/code-issues/oversized-file-create-session-pane.md`). The clients are the selected daemon's,
 * as `ProjectHomeSetupPane`'s are.
 *
 * Test ids: `sessions-project-composer-<projectId>` (the pane), `sessions-project-composer-input`,
 * `sessions-project-composer-send`, `sessions-project-composer-error`.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-orchestrator-composer.md
 */
export function OrchestratorComposer(props: OrchestratorComposerProps): React.ReactElement | null {
  // TODO(orchestrator-composer): implement
  void props;
  return null;
}
