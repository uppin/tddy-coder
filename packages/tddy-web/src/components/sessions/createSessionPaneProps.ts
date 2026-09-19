/**
 * The new-session form's contract with its callers: the services it is given, the callbacks it
 * answers on, and the optional pre-fill a caller that already knows what the session should look
 * like hands it. Lifted out of `CreateSessionPane.tsx` unchanged — `CreateSessionPane` re-exports
 * `CreateSessionInitialValues`, so every existing import keeps working.
 */

import type { Client } from "@connectrpc/connect";
import type { SessionService } from "../../gen/session_pb";
import type { ProjectService } from "../../gen/project_pb";
import type { CatalogService } from "../../gen/catalog_pb";
import type { SessionFilesService } from "../../gen/session_files_pb";
import type { WorktreeService } from "../../gen/worktree_pb";
import type { BranchWorktreeIntent } from "../../lib/branchConflict";
import type { InitialAttachment } from "./attachments/pendingAttachment";
import type { BaseBranchOption } from "./prstack/baseBranchChoice";
import type { SessionType } from "./createSessionRequest";

type ConnectionClient = Client<typeof SessionService>;
type ProjectClient = Client<typeof ProjectService>;
type CatalogClient = Client<typeof CatalogService>;
type SessionFilesClient = Client<typeof SessionFilesService>;
type WorktreeClient = Client<typeof WorktreeService>;

type BranchIntent = BranchWorktreeIntent;

/**
 * Optional pre-fill for the form's fields. Used when the pane is opened from a context that already
 * knows what the session should look like (e.g. the PR-stack "Start session" flow pre-fills the
 * branch, prompt, and stack parent). Any field left unset keeps the form's own default.
 */
export type CreateSessionInitialValues = Partial<{
  sessionType: SessionType;
  projectId: string;
  recipe: string;
  model: string;
  permissionMode: string;
  dangerouslySkipPermissions: boolean;
  stackParent: string;
  /**
   * The planned node in {@link stackParent}'s stack that this session materializes. Sent as
   * `StartSessionRequest.stack_node_id` so the daemon links the node by identity instead of matching
   * on the branch the spawn creates (D34) — the operator can rename that branch in this very form
   * before confirming, and a daemon on another host than the orchestrator cannot read its stack to
   * derive anything from it. Empty for every other caller, which keeps the branch-derived local
   * lookup the agent's own `spawn-child` relies on.
   */
  stackNodeId: string;
  branchIntent: BranchIntent;
  newBranchName: string;
  /**
   * Existing branch to pre-select in "Work on existing branch" mode — e.g. the branch a planned PR
   * already owns, which is resumed rather than re-created. Survives the async `ListProjectBranches`
   * load, which would otherwise auto-select the project's first branch.
   *
   * Named the way the rest of the domain names a branch (`feature/x`); the picker's own options are
   * remote-tracking refs (`origin/feature/x`) and are matched on the local name behind them.
   */
  selectedBranch: string;
  /** Pre-check state for the "Create Remote Branch" toggle (new-branch mode). Defaults to checked. */
  createRemoteBranch: boolean;
  /** Concrete base branch shown in the new-branch option: "New branch from base: <baseBranchLabel>". */
  baseBranchLabel: string;
  /**
   * Ordered base-branch options for the "Base branch" selector (planned-PR child sessions). Each
   * option carries the ref it submits and its caption separately: a legacy project's project default
   * is the empty ref the daemon resolves itself, which needs a label naming it rather than a blank
   * option (see `baseBranchChoice`).
   */
  baseBranchOptions: BaseBranchOption[];
  /**
   * Pre-selected base branch in the "Base branch" selector — the caller's derived base, which is
   * always one of `baseBranchOptions`.
   */
  selectedBaseBranch: string;
  initialPrompt: string;
  daemonInstanceId: string;
  /**
   * Documents the form opens with already attached — a *default* the operator can drop, not an
   * invariant. Used by the PR-stack Start-session flow, which pre-attaches the planned node's own
   * PRD and changeset plus the stack's shared documents (docs/ft/coder/pr-stack-docs.md).
   */
  attachments: InitialAttachment[];
}>;

export interface CreateSessionPaneProps {
  client: ConnectionClient;
  /** `project.ProjectService` on the same host as `client` — project registry reads. */
  projectClient: ProjectClient;
  /** `catalog.CatalogService` on the same host as `client` — tools, agents and model probes. */
  catalogClient: CatalogClient;
  /**
   * The session-files service on the same host as `client` — the form stages its local attachments
   * and lists the upload scope through it. Required for the reason `worktreeClient` is: without one
   * a staged upload silently has nowhere to go.
   */
  sessionFilesClient: SessionFilesClient;
  /**
   * The worktree service on the same host as `client` — the host-document picker's tree scopes
   * browse through it. Required for the reason `HostDocumentPicker.worktreeClient` is: without one
   * the tree scopes list nothing, and nothing is what an empty worktree looks like.
   */
  worktreeClient: WorktreeClient;
  sessionToken: string;
  onCancel: () => void;
  onCreated: (sessionId: string) => void;
  initialValues?: CreateSessionInitialValues;
}
