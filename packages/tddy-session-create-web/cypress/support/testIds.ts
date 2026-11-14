/**
 * Centralised data-testid constants.
 *
 * Every `data-testid` value used by the Cypress suite lives here — the raw string
 * appears once; tests use the named constant so a rename is a one-line change.
 *
 * For identifiers that include a dynamic segment (session ID, project ID, …) the
 * constant is a prefix or a helper function — see the examples below.
 */

import { safeTestIdPart } from "tddy-client-web";

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/** Build a `cy.get` selector for `[data-testid='<id>']`. */
export const byTestId = (
  id: string,
  options?: Parameters<typeof cy.get>[1],
): Cypress.Chainable<JQuery<HTMLElement>> =>
  cy.get(`[data-testid='${id}']`, options);

// ---------------------------------------------------------------------------
// Auth / App shell
// ---------------------------------------------------------------------------

export const TEST_IDS = {
  createSessionPane: "create-session-pane",
  /** The start's step list (`StartPhase`, `CloneProgress`), shown while a creation is streaming. */
  createSessionStartProgress: "create-session-start-progress",
  createSessionStartProgressHeading: "create-session-start-progress-heading",
  createSessionCloneProgress: "create-session-clone-progress",
  /** The session header's code-index indicator — "Indexing — <phase> <n>%" or the warm's failure. */
  sessionIndexingIndicator: "session-indexing-indicator",
  /** Host <select> — which daemon/host runs the session (multi-daemon). Rendered only when the
   *  common room advertises at least one daemon. */
  createSessionHostSelect: "create-session-host-select",
  createSessionTypeToolBtn: "create-session-type-tool",
  createSessionTypeClaudeCliBtn: "create-session-type-claude-cli",
  createSessionTypeCursorCliBtn: "create-session-type-cursor-cli",
  createSessionTypeOpenCodeCliBtn: "create-session-type-opencode-cli",
  createSessionTypeMuseCliBtn: "create-session-type-muse-cli",
  createSessionTypeWarpCliBtn: "create-session-type-warp-cli",
  createSessionProjectSelect: "create-session-project-select",
  createSessionAgentSelect: "create-session-agent-select",
  createSessionAgentEmptyOption: "create-session-agent-empty-option",
  createSessionRecipeInput: "create-session-recipe-input",
  /** Replaces the free-text recipe input for tool sessions — a <select> with all 7 recipe options. */
  createSessionRecipeSelect: "create-session-recipe-select",
  /** Parent-picker <select> — lists sessions that act as orchestrators; tool sessions only. */
  createSessionStackParentSelect: "create-session-stack-parent-select",
  /**
   * "Base the stack on" <select> — the existing session whose branch seeds a new pr-stack
   * orchestrator's stack as its single root node. Shown only for a tool session whose recipe is
   * `pr-stack`; lists only sessions that own a branch.
   */
  createSessionPrStackBaseSessionSelect: "create-session-pr-stack-base-session-select",
  createSessionModelSelect: "create-session-model-select",
  /** Effort <select> beside Model: the agent's served `effort_levels`, an empty "agent default"
   * choice first. Absent for an agent whose spec declares no levels. */
  createSessionEffortSelect: "create-session-effort-select",
  /** The provider field; absent when the harness cannot be routed or no account can drive it. */
  createSessionProviderField: "create-session-provider-field",
  /** Where the session's harness runs: "builtin" (its default vendor) or a provider account id. */
  createSessionProviderSelect: "create-session-provider-select",
  /** Why the daemon could not list the provider accounts (`ListProviderRoutes` failed). */
  createSessionProviderError: "create-session-provider-error",
  /** Inline error shown when the model probe (ListAgentModels) fails for the selected agent. */
  createSessionModelError: "create-session-model-error",
  createSessionPermissionModeSelect: "create-session-permission-mode-select",
  /** Checkbox that sets `StartSessionRequest.dangerously_skip_permissions = true` (claude-cli only).
   * Mutually exclusive with the permission-mode select, which it disables while checked. */
  createSessionDangerouslySkipPermissionsToggle:
    "create-session-dangerously-skip-permissions-toggle",
  createSessionSandboxToggle: "create-session-sandbox-toggle",
  /** "Run the agent in tmux" checkbox (every CLI harness type): sets `StartSessionRequest.run_in_tmux`.
   * Disabled while the sandbox toggle is on — the two are refused together by the daemon. */
  createSessionTmuxToggle: "create-session-tmux-toggle",
  /** "Sandboxed codebase" checkbox — jails this session's own checkout and runs the agent beside
   * it, unconfined, with its native filesystem and shell tools withdrawn. The inverted placement of
   * docs/ft/coder/sandboxed-codebase-mode.md, served by the daemon. Mutually exclusive with the
   * Sandbox and Managed codebase toggles: all three name a placement, and a session has one. */
  createSessionSandboxedCodebaseToggle: "create-session-sandboxed-codebase-toggle",
  /** Why the sandboxed-codebase placement is unavailable on the selected host. Rendered beside a
   * disabled toggle, never in place of one — the capability stays discoverable from a host that
   * cannot serve it. */
  createSessionSandboxedCodebaseUnavailable: "create-session-sandboxed-codebase-unavailable",
  /** What the selected host's jail does *not* confine. Rendered beside an enabled toggle on a host
   * whose jail shares the filesystem root, so "sandboxed" never promises more than it delivers.
   * See docs/dev/todo/2026-06-28-tddy-sandbox-cgroups.md. */
  createSessionSandboxedCodebaseCaveat: "create-session-sandboxed-codebase-caveat",
  /** Collapsible "Managed codebase" section header — claude-cli sessions only. See
   * docs/ft/coder/specialized-subagents.md. */
  createSessionManagedCodebaseToggle: "create-session-managed-codebase-toggle",
  /** "Session orchestrator" switch — asks `StartSession` for `session_role: "orchestrator"`; offered
   * only for an agent whose spec declares `project_home_main`. On, it hides the placement, branch and
   * stack fields. See docs/ft/daemon/1-WIP/PRD-2026-11-10-orchestrator-sessions.md. */
  createSessionOrchestratorToggle: "create-session-orchestrator-toggle",
  /** Expanded "Managed codebase" section content (specialized-subagent multi-select). */
  createSessionAgentPickerSection: "create-session-agent-picker-section",
  /** "Semantic index" checkbox inside the Managed codebase section — when on, the daemon indexes the
   * worktree before launch and exposes the SemanticSearch tool. See docs/ft/coder/semantic-index.md. */
  createSessionSemanticIndexToggle: "create-session-semantic-index-toggle",
  /** "Codebase host" <select> inside the Managed codebase section — which daemon's filesystem holds
   * the session's git worktree. Empty value means "same as host" (co-located). claude-cli only.
   * See docs/ft/daemon/remote-managed-worktree.md. */
  createSessionCodebaseHostSelect: "create-session-codebase-host-select",
  /** OpenSSH Host alias the session's exec catalog runs on. Empty = this host (LocalShell). */
  createSessionSshConfigSelect: "create-session-ssh-config-select",
  createSessionInitialPromptInput: "create-session-initial-prompt-input",
  createSessionBranchIntentSelect: "create-session-branch-intent-select",
  createSessionNewBranchNameInput: "create-session-new-branch-name-input",
  createSessionBranchToWorkOnSelect: "create-session-branch-to-work-on-select",
  /** "Create Remote Branch" checkbox — unchecked by default (pre-checked for a PR-stack node); pushes the new branch to origin at session start. */
  createSessionCreateRemoteBranchToggle: "create-session-create-remote-branch-toggle",
  // Attachments — documents attached at creation time (see docs/ft/coder/session-attachments.md).
  // Local files are staged to the *connected* daemon on submit and referenced as
  // `StagedAttachmentRef`; a document already on a host is referenced as `HostDocumentRef` with no
  // upload. Both arrive as `StartSessionRequest.attachments`.
  /** Section wrapper — also the drag-drop target for OS file drops. */
  createSessionAttachmentsSection: "create-session-attachments-section",
  /** Drag-over overlay, shown only while a file drag is over the section. */
  createSessionAttachmentDropOverlay: "create-session-attachment-drop-overlay",
  /** `<label>`-wrapped native multi-file picker (the input inside is visually hidden). */
  createSessionAttachmentPickBtn: "create-session-attachment-pick-btn",
  /** Opens the host-document picker, which attaches by reference instead of uploading. */
  createSessionAttachmentPickHostDocBtn: "create-session-attachment-pick-host-doc-btn",
  /** Inline refusal shown when a picked file is unusable (over the host cap, duplicate name). */
  createSessionAttachmentError: "create-session-attachment-error",
  /** Host-document picker dialog, and one selectable row per document it lists. */
  createSessionHostDocPicker: "create-session-host-doc-picker",
  createSessionHostDocPickerScopeSelect: "create-session-host-doc-picker-scope-select",
  createSessionCancelBtn: "create-session-cancel-btn",
  createSessionSubmitBtn: "create-session-submit-btn",
  createSessionError: "create-session-error",
  // Orchestrator composer (`#orchestrators` 8/11) — opened by a project header's "+" at
  // `/sessions/project/:projectId/new`; the pane itself is `sessionsProjectComposer(projectId)`.
  /** The composer's prompt box: the orchestrator's first prompt. */
  sessionsProjectComposerInput: "sessions-project-composer-input",
  /** Starts the orchestrator (as Enter in the box does). */
  sessionsProjectComposerSend: "sessions-project-composer-send",
  /** The daemon's reason a start was refused, shown under the box. */
  sessionsProjectComposerError: "sessions-project-composer-error",

  // Sessions drawer — branch-conflict prompt. Shown when the daemon refuses the creation because
  // another session already owns the requested branch (`StartSessionResponse.branch_conflict`).
  // See docs/ft/daemon/session-branch-conflict.md.
  branchConflictDialog: "branch-conflict-dialog",
  /** Names the owning session and whether it is active — what the operator would switch to. */
  branchConflictOwner: "branch-conflict-owner",
  /** Attach to the owning session instead of creating anything. */
  branchConflictSwitchBtn: "branch-conflict-switch-btn",
  /** Start a second agent on the owned branch, sharing the owning session's worktree. */
  branchConflictAddAgentBtn: "branch-conflict-add-agent-btn",
  /** Editable branch name, pre-filled with the daemon's `suggested_branch_name`. */
  branchConflictRenameInput: "branch-conflict-rename-input",
  /** Re-submit creation under the name typed into `branchConflictRenameInput`. */
  branchConflictRenameBtn: "branch-conflict-rename-btn",
  branchConflictCancelBtn: "branch-conflict-cancel-btn",
} as const;
export const sessionsDrawerProjectGroup = (projectKey: string) =>
  `sessions-drawer-project-group-${projectKey}`;

export const sessionsDrawerProjectHome = (projectId: string) =>
  `sessions-drawer-project-home-${projectId}`;

export const sessionsProjectHomeSetup = (projectId: string) =>
  `sessions-project-home-setup-${projectId}`;

export const sessionsProjectHomeSetupUnknown = (projectId: string) =>
  `sessions-project-home-setup-unknown-${projectId}`;

export const sessionsProjectHomeSetupUnreachable = (projectId: string) =>
  `sessions-project-home-setup-unreachable-${projectId}`;

export const sessionsDrawerProjectHomeReplace = (projectId: string) =>
  `sessions-drawer-project-home-replace-${projectId}`;

export const sessionsProjectHomeSetupReplacing = (projectId: string) =>
  `sessions-project-home-setup-replacing-${projectId}`;

export const createSessionTypeReason = (sessionType: string) =>
  `create-session-type-reason-${sessionType}`;

export const projectMainSessionLink = (projectId: string) =>
  `project-main-session-link-${projectId}`;

export const worktreeTreeNode = (relPath: string) => `worktree-tree-node-${relPath}`;

export const createSessionAttachmentRow = (basename: string) =>
  `create-session-attachment-row-${basename}`;

export const createSessionAttachmentName = (basename: string) =>
  `create-session-attachment-name-${basename}`;

export const createSessionAttachmentSize = (basename: string) =>
  `create-session-attachment-size-${basename}`;

export const createSessionAttachmentRemove = (basename: string) =>
  `create-session-attachment-remove-${basename}`;

export const createSessionAttachmentProgress = (basename: string) =>
  `create-session-attachment-progress-${basename}`;

export const createSessionHostDocRow = (relativePath: string) =>
  `create-session-host-doc-row-${relativePath}`;

// ---------------------------------------------------------------------------
// Models & Agents dynamic helpers
// ---------------------------------------------------------------------------

export const createSessionAgentOption = (agentId: string) =>
  `create-session-agent-${safeTestIdPart(agentId)}`;

export const createSessionAgentOptionHost = (agentId: string) =>
  `${createSessionAgentOption(agentId)}-host`;

export const createSessionAgentOptionKind = (agentId: string) =>
  `${createSessionAgentOption(agentId)}-kind`;

export const createSessionAgentHostError = (daemonInstanceId: string) =>
  `create-session-agent-host-error-${safeTestIdPart(daemonInstanceId)}`;

export const createSessionAgentSelectOption = (optionValue: string) =>
  `create-session-agent-select-option-${safeTestIdPart(optionValue)}`;

export const createSessionAgentSelectHostError = (daemonInstanceId: string) =>
  `create-session-agent-select-host-error-${safeTestIdPart(daemonInstanceId)}`;


/** `[data-testid="sessions-project-composer-<projectId>"]` — the orchestrator composer for a project
 *  (`#orchestrators` 8/11), opened by its header's "+" at `/sessions/project/:projectId/new`. */
export const sessionsProjectComposer = (projectId: string) =>
  `sessions-project-composer-${projectId}`;
