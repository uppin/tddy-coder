import React from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionEntry, SessionService } from "tddy-client-web/gen/session_pb";
import type { ProjectService } from "tddy-client-web/gen/project_pb";
import type { CatalogService } from "tddy-client-web/gen/catalog_pb";
import type { PrStackService, StackAddress } from "tddy-client-web/gen/pr_stack_pb";
import type { SessionFilesService } from "tddy-client-web/gen/session_files_pb";
import type { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import type { TaskService } from "tddy-client-web/gen/tasks_pb";
import type { StackChildMetadata } from "./stackChildSessions";

/** The fields of a `pr_stack.StackAddress` the view is opened on. */
export type ProjectStackAddress = Pick<StackAddress, "projectId" | "stackId" | "daemonInstanceId">;

export interface ProjectStackViewProps {
  /**
   * The stack to show, as the drawer's Stacks row listed it (`ListProjectStacks`): every request the
   * view makes carries this address, and the daemon it names serves them — forwarded there by
   * whichever daemon the view's clients reach.
   */
  address: ProjectStackAddress;
  /** `pr_stack.PrStackService` — `GetProjectStack`, `QueryBranch` and every planned-PR control. */
  prStackClient?: Client<typeof PrStackService>;
  sessionToken?: string;
  /** The full session list (all hosts) — each planned node's child session is resolved from it. */
  sessions?: SessionEntry[];
  /** The `session` metadata block each live participant publishes, keyed by session id (D37, D38). */
  sessionMetadataBySessionId?: ReadonlyMap<string, StackChildMetadata>;
  /** The project's default branch (`ProjectEntry.main_branch_ref`); empty for a legacy project (D20). */
  defaultBranch?: string;
  /** The project's default remote (`ProjectEntry.default_remote`); the view falls back to `origin`. */
  defaultRemote?: string;
  /** Select and attach a session — how a spawned row's status chip opens its child. */
  onOpenSession?: (sessionId: string) => void;
  /** Fired after a child session is spawned from a row, so the caller can show it in the drawer. */
  onChildSessionStarted?: (entry: { sessionId: string; recipe: string; projectId: string }) => void;
  /** The Start-session dialog's clients, on the host the stack lives on. The dialog is offered only with all five. */
  client?: Client<typeof SessionService>;
  projectClient?: Client<typeof ProjectService>;
  catalogClient?: Client<typeof CatalogService>;
  sessionFilesClient?: Client<typeof SessionFilesService>;
  worktreeClient?: Client<typeof WorktreeService>;
  taskClient?: Client<typeof TaskService>;
}

/**
 * The **stack view** (`#orchestrators` 9/11): one of a project's PR stacks, opened by its id from the
 * drawer's Stacks row at `/sessions/project/:projectId/stack/:stackId`. Today's Planned PRs panel at
 * full width — the planned-PR list with every control it has (start session, repoint, reorder, add
 * planned PR, pull base) — loaded with `GetProjectStack` and acting on the stack by its address, so it
 * needs no orchestrator session at all.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-stack-web-by-id.md
 */
export function ProjectStackView(props: ProjectStackViewProps): React.ReactElement {
  // TODO(stack-web-by-id): implement — load the stack with `GetProjectStack`, poll `QueryBranch` by
  // address, and render `PlannedPrList` with the add form, repoint, reorder, pull and start controls.
  const { address } = props;
  return (
    <div
      data-testid="project-stack-view"
      data-stack-id={address.stackId}
      className="flex-1 min-h-0 flex flex-col overflow-hidden"
    />
  );
}
