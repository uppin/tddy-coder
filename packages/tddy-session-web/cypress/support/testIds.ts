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
// Session runtime and terminal
// ---------------------------------------------------------------------------

export const TEST_IDS = {
  controlHolderDisplay: "control-holder-display",
  controlIsControllerDisplay: "control-is-controller-display",
  controlTokenDisplay: "control-token-display",
  enqueuedInputOverlay: "enqueued-input-overlay",
  enqueuedInputText: "enqueued-input-text",
  terminal: "terminal",
  loadEarlierHistory: "load-earlier-history",
  sessionConnectionError: "session-connection-error",
  sessionConnectionOverlay: "session-connection-overlay",
  switchSession: "switch-session",
  terminalBufferText: "terminal-buffer-text",
  terminalClaimBtn: "terminal-claim-btn",
  terminalControlHolder: "terminal-control-holder",
  terminalControlOverlay: "terminal-control-overlay",
  terminalInputError: "terminal-input-error",
  terminalOlderBufferText: "terminal-older-buffer-text",
  agentActivityButton: "agent-activity-button",
  /** Unread-activity badge on the icon — shown while `unreadCount > 0`. */
  agentActivityDetailClose: "agent-activity-detail-close",
  /** The tool call's raw_input, prettified + color-highlighted JSON. */
  agentActivityDetailDialog: "agent-activity-detail-dialog",
  /** Close control on the detail dialog. */
  agentActivityDetailError: "agent-activity-detail-error",

  // PR-Stack Chat Screen — manually adding a planned PR (deterministic, non-chat path)
  agentActivityDetailInput: "agent-activity-detail-input",
  /** The tool call's raw_output, prettified + color-highlighted JSON. */
  agentActivityDetailNoInput: "agent-activity-detail-no-input",
  /** Inline error shown when the detail lookup cannot be answered (entry carries no tool call id,
   *  unknown id, or transport failure). */
  agentActivityDetailNoOutput: "agent-activity-detail-no-output",
  /** Note shown under the Input heading when the lookup resolved without an input body. */
  agentActivityDetailOutput: "agent-activity-detail-output",
  /** A color-highlighted JSON block (Prism output) inside the detail dialog. */
  agentActivityDetailSkeleton: "agent-activity-detail-skeleton",
  /** Note shown under the Output heading when the call has produced no output yet. */
  agentActivityJsonHighlight: "agent-activity-json-highlight",
  /** Placeholder shown in place of a JSON body while `GetAcpToolCallDetail` is in flight. */
  agentActivityOverlay: "agent-activity-overlay",
  /** The Agent Activity transcript docked as the session view's right sidebar (`#project-home` 3/7). */
  agentActivityOverlayClose: "agent-activity-overlay-close",
  /** Tool-call detail dialog opened by clicking a tool entry in the transcript. */
  agentActivitySidebar: "agent-activity-sidebar",
  /** Close control on the overlay. */
  agentActivityUnreadBadge: "agent-activity-unread-badge",
  /** The in-pane overlay listing one-line records. */
  agentConversationError: "agent-conversation-error",

  // Shell navigation
  agentConversationInput: "agent-conversation-input",
  /** Sends what is in `agentConversationInput`. */
  agentConversationSendBtn: "agent-conversation-send-btn",
  /** A failed open or a failed prompt, named. Never an empty transcript standing in for one. */
  agentConversationTranscript: "agent-conversation-transcript",
  /** Where the operator types the next prompt. */
  agentRosterAddBtn: "agent-roster-add-btn",
  /** The picker dialog opened by `agentRosterAddBtn`. */
  agentRosterDetachConfirm: "agent-roster-detach-confirm",
  /** Accepts `agentRosterDetachConfirm`. */
  agentRosterDetachConfirmBtn: "agent-roster-detach-confirm-btn",

  // Agent tree — the Agents tab as a hierarchy: the session's main agent at the root, its roster
  // agents and its subagent sessions beneath it
  // (docs/ft/daemon/session-agent-roster.md § The Agents tab).
  /** The tree's outermost list. Replaces the roster pane's flat `<ul>`. */
  agentRosterDisconnected: "agent-roster-disconnected",
  /** Rendered when the roster could not be read — distinct from "no agents attached". */
  agentRosterEmpty: "agent-roster-empty",
  /** Opens the fanned-out agent picker. */
  agentRosterError: "agent-roster-error",
  /** Rendered when the roster is genuinely empty. */
  agentRosterLoading: "agent-roster-loading",
  /** Rendered when the session's daemon is not connected. */
  agentRosterPane: "agent-roster-pane",
  /** Rendered while the first roster frame has not arrived. */
  agentRosterPicker: "agent-roster-picker",
  /** Warning naming the tools the main agent loses if the highlighted agent is attached. */
  agentRosterPickerConfirmBtn: "agent-roster-picker-confirm-btn",
  /** Confirmation shown before a detach that deletes a checkout on another host. */
  agentRosterPickerWithdrawalWarning: "agent-roster-picker-withdrawal-warning",
  /** Confirms the attach of the agent selected in the picker. */
  agentTree: "agent-tree",
  /** The root row — the session's own main agent. */
  agentTreeRoot: "agent-tree-root",
  /** What the main agent is doing; carries `data-agent-status`. */
  agentTreeRootChildren: "agent-tree-root-children",

  // Agent conversation — attaching a roster agent from the session header and talking to it in a
  // tab (docs/ft/web/session-drawer.md § Add agent).
  /** The "Add agent" button in the session-detail header. Attaches a roster agent to the CURRENT
   *  session — it does not spawn a peer session, which is what it used to do. */
  agentTreeRootLastActivity: "agent-tree-root-last-activity",
  /** The list holding the main agent's children — its roster agents and its subagent sessions. */
  agentTreeRootStatus: "agent-tree-root-status",
  /** The main agent's last observed activity. Absent when nothing has been observed of it. */
  livekitStatus: "livekit-status",
  restructurePlanDialog: "restructure-plan-dialog",
  restructurePlanRun: "restructure-plan-run",
  restructurePlanStaleNotice: "restructure-plan-stale-notice",
  restructurePlanTitle: "restructure-plan-title",
  sessionAgentAttachBtn: "session-agent-attach-btn",
  /** The header's fanned-out agent picker. The same component as the roster pane's picker, under
   *  its own id prefix so the two can be open at once without colliding. */
  sessionAgentAttachError: "session-agent-attach-error",

  /** The conversation tab's body: transcript plus composer. */
  sessionAgentPicker: "session-agent-picker",
  /** Warning naming the tools the main agent loses if the highlighted agent is attached. */
  sessionAgentPickerConfirmBtn: "session-agent-picker-confirm-btn",
  /** Dismisses the header picker without attaching. */
  sessionAgentPickerWithdrawalWarning: "session-agent-picker-withdrawal-warning",
  /** Confirms the attach of the agent selected in the header picker. */
  sessionFilePreview: "session-file-preview",
  sessionFilesEmpty: "session-files-empty",

  // CodexOAuth dialog
  sessionFilesPanel: "session-files-panel",
  sessionIndexingIndicator: "session-indexing-indicator",
  /** Host <select> — which daemon/host runs the session (multi-daemon). Rendered only when the
   *  common room advertises at least one daemon. */
  sessionInspectorTabAgents: "sessions-inspector-tab-agents",
  sessionInspectorTabFiles: "sessions-inspector-tab-files",
  sessionMoreActionsShowFiles: "session-more-actions-show-files",

  // Worktree Code pane (docs/ft/web/session-code-pane.md)
  sessionsDetailPane: "sessions-detail-pane",
  sessionsTerminalFullscreen: "sessions-terminal-fullscreen",
  /** The floating "exit full screen" control drawn over a pane stack that holds fullscreen. */
  sessionsTerminalFullscreenExit: "sessions-terminal-fullscreen-exit",

  // Session connection overlay — covers the runtime's panes until LiveKit is connected
  sessionsTerminalTabAgent: "sessions-terminal-tab-agent",
  /** The "+" button that opens a new bash terminal. */
  sessionsTerminalTabNew: "sessions-terminal-tab-new",
  sessionsTerminalTabNewMenu: "sessions-terminal-tab-new-menu",
  /** The trailing ⛶ control that puts the active pane into browser full screen. */
  sessionsTerminalTabs: "sessions-terminal-tabs",
  /** The fixed, non-closable Agent tab (reserved "main" terminal). */
  sessionsToolCallInput: "sessions-tool-call-input",
  sessionsToolCallLog: "sessions-tool-call-log",
  sessionsToolCallOutput: "sessions-tool-call-output",
  sessionsToolCallRow: "sessions-tool-call-row",
  sessionsToolCallStdio: "sessions-tool-call-stdio",

  // Tasks drawer screen
  sessionsToolInvokeArgs: "sessions-tool-invoke-args",
  sessionsToolInvokeButton: "sessions-tool-invoke-button",
  sessionsToolInvokeError: "sessions-tool-invoke-error",

  // Session inspector — Tools tab: call log
  sessionsToolInvokeResult: "sessions-tool-invoke-result",
  sessionsToolInvokeSelect: "sessions-tool-invoke-select",
  shortcutDrawer: "shortcut-drawer",
  worktreeCodeHighlight: "worktree-code-highlight",
  // Code navigation in the preview (docs/ft/web/session-code-pane.md)
  worktreeCodeHover: "worktree-code-hover",
  /** The hover card's scrolling body: the rendered hover markdown. */
  worktreeCodeHoverBody: "worktree-code-hover-body",
  /** The hover card's loading state while its query is held: `Index loading — <phase> <n>%`. */
  worktreeCodeHoverClose: "worktree-code-hover-close",
  /** The navigation notice: go-to-definition outcome, held state, or failure. */
  worktreeCodeHoverLoading: "worktree-code-hover-loading",
  /** The hover card's Close action (in its non-scrolling footer, beside References). */
  worktreeCodeIndexStatus: "worktree-code-index-status",
  /** The strip's headline — phase and percentage, ready, or failed. */
  worktreeCodeIndexStatusDetail: "worktree-code-index-status-detail",
  /** The strip's `+N more` when several index phases run at once; its `title` lists every phase. */
  worktreeCodeIndexStatusMore: "worktree-code-index-status-more",
  worktreeCodeIndexStatusSummary: "worktree-code-index-status-summary",
  /** The strip's detail — the message of the phase the headline names. */
  worktreeCodeNotice: "worktree-code-notice",
  worktreeCodeOpenAsPlan: "worktree-code-open-as-plan",
  worktreeCodePane: "worktree-code-pane",
  /** The Code pane's bottom strip: the session's code-index status (`data-state` names it). */
  worktreeCodeReferences: "worktree-code-references",
  // Restructure plan dialog (docs/ft/web/1-WIP/PRD-2026-10-03-plan-dialog.md)
  worktreeCodeReferencesAction: "worktree-code-references-action",
  worktreeCodeToggle: "sessions-code-toggle",
  worktreeFilePreview: "worktree-file-preview",
  worktreeFileTree: "worktree-file-tree",
  yamlSyntaxHighlight: "yaml-syntax-highlight",

  // Session more-actions menu
  sessionInspectorTabWorktree: "sessions-inspector-tab-worktree",
  sessionsDetailTerminalContainer: "sessions-detail-terminal-container",
  sessionsInspectorMetadata: "sessions-inspector-metadata",

  // A stopped session's Agent tab — the greyed recorded output and its Resume toast
  sessionsInspectorTabAgents: "sessions-inspector-tab-agents",
  sessionsInspectorTabDetails: "sessions-inspector-tab-details",
  sessionsInspectorTabScreenSharing: "sessions-inspector-tab-screen-sharing",
  sessionsInspectorTabTools: "sessions-inspector-tab-tools",
  /** Opens the session's agent roster (`agentRosterPane`). */
  sessionsInspectorTabUsage: "sessions-inspector-tab-usage",
  sessionsInspectorTabVnc: "sessions-inspector-tab-vnc",
  sessionsScreenSharingTabPanel: "sessions-screen-sharing-tab-panel",
  sessionsVncTabPanel: "sessions-vnc-tab-panel",
  // session-drawer (#862): moved with the drawer-driven specs
  // Terminal chrome
  connectionStatusDot: "connection-status-dot",
  terminalConnectionStatusBar: "terminal-connection-status-bar",
  sessionWorktreeTab: "session-worktree-tab",
  sessionWorktreeSize: "session-worktree-size",
  sessionWorktreeLastCalculated: "session-worktree-last-calculated",
  sessionWorktreeBranch: "session-worktree-branch",
  sessionWorktreeRefresh: "session-worktree-refresh",
  sessionWorktreeClear: "session-worktree-clear",
  sessionWorktreeClearConfirm: "session-worktree-clear-confirm",
  sessionWorktreeDelete: "session-worktree-delete",
  sessionWorktreeDeleteConfirm: "session-worktree-delete-confirm",
  sessionWorktreeMissing: "session-worktree-missing",
  sessionWorktreeRestore: "session-worktree-restore",
  // Sessions drawer screen
  sessionsDrawerScreen: "sessions-drawer-screen",
  sessionsDrawer: "sessions-drawer",
  /** The drawer's footnote when it cannot see sessions owned by other hosts. */
  sessionsDrawerCrossHostUnavailable: "sessions-drawer-cross-host-unavailable",
  sessionsDetailMetadata: "sessions-detail-metadata",
  // Session activities view — the inactive session's default main-pane view (recorded ACP transcript)
  sessionsActivitiesPane: "sessions-activities-pane",
  sessionsActivitiesEmpty: "sessions-activities-empty",
  // Session inspector drawer
  sessionsInspectorDrawer: "sessions-inspector-drawer",
  sessionsInspectorToggle: "sessions-inspector-toggle",
  sessionsInspectorClose: "sessions-inspector-close",
  sessionsInspectorExpand: "sessions-inspector-expand",
  sessionsInspectorRestore: "sessions-inspector-restore",
  // A stopped session's Agent tab — the greyed recorded output and its Resume toast
  dormantAgentTerminal: "dormant-agent-terminal",
  dormantAgentTerminalEmpty: "dormant-agent-terminal-empty",
  dormantAgentToast: "dormant-agent-toast",
  dormantAgentToastAction: "dormant-agent-toast-action",
  dormantAgentToastError: "dormant-agent-toast-error",
  sessionsInspectorTabActivity: "sessions-inspector-tab-activity",
  /** "Select" button that activates selection mode (reveals per-row checkboxes). */
  sessionsDrawerSelectMode: "sessions-drawer-select-mode",
  /** "Select all" / "Deselect all" toggle, shown while in selection mode. */
  sessionsDrawerSelectAll: "sessions-drawer-select-all",
  /** Single "delete selected" action button, enabled once ≥1 row checkbox is ticked. */
  sessionsDrawerBulkDelete: "sessions-drawer-bulk-delete",
  /** Why the selected host's session list could not be loaded (the last good list stays shown). */
  sessionsDrawerLoadError: "sessions-drawer-load-error",
  /** Why the selected session's last pane action (attach, resume, delete, terminate) failed. */
  sessionsPaneError: "sessions-pane-error",
  /** Why a manual "Claim terminal" did not take control. */
  terminalClaimError: "terminal-claim-error",
  // Sessions drawer — active / remaining partition separators (collapsible headers)
  /** "Active (N)" partition header — expanded by default; toggles the active rows below it. */
  sessionsDrawerSeparatorActive: "sessions-drawer-separator-active",
  /** "Remaining (M)" partition header — collapsed by default; toggles the disconnected rows below it. */
  sessionsDrawerSeparatorRemaining: "sessions-drawer-separator-remaining",
  // Sessions drawer — open/close toggle
  sessionsDrawerCloseBtn: "sessions-drawer-close-btn",
  sessionsDrawerOpenBtn: "sessions-drawer-open-btn",
  sessionsDrawerOpenOverlayBtn: "sessions-drawer-open-overlay-btn",
  // Sessions drawer — create session
  sessionsDrawerNewBtn: "sessions-drawer-new-btn",
  createSessionPane: "create-session-pane",
  /** The start's step list (`StartPhase`, `CloneProgress`), shown while a creation is streaming. */
  createSessionStartProgress: "create-session-start-progress",
  createSessionStartProgressHeading: "create-session-start-progress-heading",
  createSessionCloneProgress: "create-session-clone-progress",
  /** Host <select> — which daemon/host runs the session (multi-daemon). Rendered only when the
   *  common room advertises at least one daemon. */
  createSessionHostSelect: "create-session-host-select",
  /** Overlay dialog wrapping CreateSessionPane, used by the PR-stack "Start session" flow. */
  createSessionDialog: "create-session-dialog",
  createSessionTypeToolBtn: "create-session-type-tool",
  createSessionTypeClaudeCliBtn: "create-session-type-claude-cli",
  createSessionTypeCursorCliBtn: "create-session-type-cursor-cli",
  createSessionTypeOpenCodeCliBtn: "create-session-type-opencode-cli",
  createSessionTypeMuseCliBtn: "create-session-type-muse-cli",
  createSessionProjectSelect: "create-session-project-select",
  createSessionAgentSelect: "create-session-agent-select",
  createSessionAgentEmptyOption: "create-session-agent-empty-option",
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
  /** Why the Project picker has nothing to offer: `ListProjects` failed. Never an empty list. */
  createSessionProjectsLoadError: "create-session-projects-load-error",
  /** Why no tool is selected for a tool session: `ListTools` failed. */
  createSessionToolsLoadError: "create-session-tools-load-error",
  /** Why the stack-parent and stack-base pickers have nothing to offer: `ListSessions` failed. */
  createSessionSessionsLoadError: "create-session-sessions-load-error",
  /** Why the "Branch to work on" picker has nothing to offer: `ListProjectBranches` failed. */
  createSessionBranchesLoadError: "create-session-branches-load-error",
  createSessionPermissionModeSelect: "create-session-permission-mode-select",
  /** Checkbox that sets `StartSessionRequest.dangerously_skip_permissions = true` (claude-cli only).
   * Mutually exclusive with the permission-mode select, which it disables while checked. */
  createSessionDangerouslySkipPermissionsToggle:
    "create-session-dangerously-skip-permissions-toggle",
  createSessionSandboxToggle: "create-session-sandbox-toggle",
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
  /** Base-branch <select> for a planned-PR child session — lists the node's direct dependency branches
   *  first (ordered by dependency depth, deepest first, ties by `node.parents` order), then the stack's
   *  other materialized branches. Shown only when `initialValues.stackParent` is set and not peer mode.
   *  The selected value is sent as `StartSessionRequest.selected_integration_base_ref`. */
  createSessionBaseBranchSelect: "create-session-base-branch-select",
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
  shellMenuHosts: "shell-menu-hosts",
  /** The table of known hosts. Its `<tbody>` rows are the rows. */
  hostsTable: "hosts-table",
  // Terminal routes
  terminalRouteUnknownSession: "terminal-route-unknown-session",
  terminalRouteUnknownSessionHome: "terminal-route-unknown-session-home",
  // Host stats footer (screen-level bottom strip: relocated traffic + disk + per-core CPU)
  hostStatsFooter: "host-stats-footer",
  /** Free-space readout for the daemon's default project directory filesystem. */
  diskSpaceAvailable: "disk-space-available",
  /** Container holding one mini bar per logical core. */
  cpuCores: "cpu-cores",
  // Session traffic strip
  sessionTrafficStrip: "session-traffic-strip",
  sessionTrafficBytesIn: "session-traffic-bytes-in",
  sessionTrafficBytesOut: "session-traffic-bytes-out",
  sessionTrafficRateIn: "session-traffic-rate-in",
  sessionTrafficRateOut: "session-traffic-rate-out",
  sessionTrafficPing: "session-traffic-ping",
  // Fast Session Change — per-session runtime registry + background terminals
  /** The hidden layer that keeps one mounted terminal per attached session. */
  sessionsRuntimeLayer: "sessions-runtime-layer",
  /** Inspector Details tab — cumulative inbound bytes for the session. */
  sessionsInspectorBytesIn: "sessions-inspector-bytes-in",
  /** Inspector Details tab — cumulative outbound bytes for the session. */
  sessionsInspectorBytesOut: "sessions-inspector-bytes-out",
  /** Inspector Details tab — "last data received: Ns ago" relative timestamp. */
  sessionsInspectorLastDataReceived: "sessions-inspector-last-data-received",
  /** Drawer row — container for parsed `session` participant metadata (goal/state/agent/model). */
  sessionsDrawerItemSessionMeta: "sessions-drawer-item-session-meta",
  sessionsUsageTabPanel: "sessions-usage-tab-panel",
  sessionsUsageEmpty: "sessions-usage-empty",
  sessionsUsageTotalInput: "sessions-usage-total-input",
  sessionsUsageTotalCacheRead: "sessions-usage-total-cache-read",
  sessionsUsageTotalCacheWrite: "sessions-usage-total-cache-write",
  sessionsUsageTotalReasoning: "sessions-usage-total-reasoning",
  sessionsUsageTotalCost: "sessions-usage-total-cost",
  sessionsUsageTotalOutput: "sessions-usage-total-output",
  sessionsUsageTotalTotal: "sessions-usage-total-total",
  /** Why the usage stream stopped (or never opened); the last snapshot stays shown beside it. */
  sessionsUsageStreamError: "sessions-usage-stream-error",
  /** Why the drawer's project registry could not be read (unscoped sessions cannot resolve one). */
  sessionsDrawerProjectsLoadError: "sessions-drawer-projects-load-error",
  /** Why the daemon's notification feed stopped — the row dots are no longer live. */
  sessionsDrawerNotificationsError: "sessions-drawer-notifications-error",
  /** Why an activity stream stopped, as `useSessionActivity` reports it. */
  sessionActivityStreamError: "session-activity-stream-error",
  sessionsVncTargetList: "sessions-vnc-target-list",
  sessionsVncAddForm: "sessions-vnc-add-form",
  sessionsVncAddLabel: "sessions-vnc-add-label",
  sessionsVncAddHost: "sessions-vnc-add-host",
  sessionsVncAddPort: "sessions-vnc-add-port",
  sessionsVncAddPassword: "sessions-vnc-add-password",
  sessionsVncAddSubmit: "sessions-vnc-add-submit",
  sessionsVncPassphraseDialog: "sessions-vnc-passphrase-dialog",
  sessionsVncPassphraseInput: "sessions-vnc-passphrase-input",
  sessionsVncPassphraseConfirm: "sessions-vnc-passphrase-confirm",
  // VNC overlay
  vncOverlay: "vnc-overlay",
  vncOverlayVideo: "vnc-overlay-video",
  vncOverlayClose: "vnc-overlay-close",
  sessionsScreenSharingTargetList: "sessions-screen-sharing-target-list",
  sessionsScreenSharingAddForm: "sessions-screen-sharing-add-form",
  sessionsScreenSharingAddLabel: "sessions-screen-sharing-add-label",
  sessionsScreenSharingAddHost: "sessions-screen-sharing-add-host",
  sessionsScreenSharingAddPort: "sessions-screen-sharing-add-port",
  sessionsScreenSharingAddPassword: "sessions-screen-sharing-add-password",
  sessionsScreenSharingAddProtocol: "sessions-screen-sharing-add-protocol",
  sessionsScreenSharingAddSubmit: "sessions-screen-sharing-add-submit",
  /** Shown when the credential store exists and this session's key does not open it. */
  sessionsScreenSharingVaultLocked: "sessions-screen-sharing-vault-locked",
  // Session inspector tabs — why a load or an action failed, shown beside what failed
  sessionsVncListError: "sessions-vnc-list-error",
  sessionsVncAddError: "sessions-vnc-add-error",
  sessionsScreenSharingListError: "sessions-screen-sharing-list-error",
  sessionFilesError: "session-files-error",
  sessionWorktreeError: "session-worktree-error",
  sessionsToolListError: "sessions-tool-list-error",
  sessionsToolCallLogError: "sessions-tool-call-log-error",
  /** Why listing, opening or closing a bash terminal failed — under the terminal tab strip. */
  sessionsTerminalError: "sessions-terminal-error",
  // Screen Sharing overlay
  screenSharingOverlay: "screen-sharing-overlay",
  screenSharingOverlayVideo: "screen-sharing-overlay-video",
  screenSharingOverlayClose: "screen-sharing-overlay-close",
  // PR-Stack Chat Screen (per-workflow session view for the "pr-stack" recipe)
  prStackScreen: "pr-stack-screen",
  prStackPlannedPrList: "pr-stack-planned-pr-list",
  // PR-Stack "Planned PRs" panel — right-side dock on desktop, full-screen overlay on mobile.
  // Always mounted; `data-state` ∈ {closed, open} drives visibility (same contract as the
  // Session Inspector drawer).
  prStackPlannedPrPanel: "pr-stack-planned-pr-panel",
  prStackPlannedPrPanelToggle: "pr-stack-planned-pr-panel-toggle",
  prStackPlannedPrPanelClose: "pr-stack-planned-pr-panel-close",
  // Full-screen Workflow Chat Screen (per-workflow session view for every non-"pr-stack" tool recipe)
  workflowChatScreen: "workflow-chat-screen",
  // Reusable Agent Chat (recipe-agnostic; the PR-Stack chat view renders it via PrStackChat)
  agentChat: "agent-chat",
  agentChatMessages: "agent-chat-messages",
  agentChatInput: "agent-chat-input",
  agentChatSendBtn: "agent-chat-send-btn",
  agentChatExportBtn: "agent-chat-export-btn",
  agentChatError: "agent-chat-error",
  agentChatConnecting: "agent-chat-connecting",
  agentChatStatus: "agent-chat-status",
  agentChatQuestion: "agent-chat-question",
  agentChatQuestionHeader: "agent-chat-question-header",
  agentChatQuestionText: "agent-chat-question-text",
  agentChatQuestionOtherInput: "agent-chat-question-other-input",
  agentChatQuestionOtherSubmit: "agent-chat-question-other-submit",
  agentChatMultiSelectSubmit: "agent-chat-multiselect-submit",
  // Read-only transcript scroll behaviour (tail-first open, auto-follow, backwards paging)
  /** Affordance shown while the reader has scrolled away from the newest entry; its text carries the
   *  number of entries that arrived since detaching. Clicking it returns to the newest entry. */
  agentChatJumpToLatest: "agent-chat-jump-to-latest",
  /** Top-edge indicator shown while an older page (`GetAcpReplayPage`) is in flight. */
  agentChatOlderLoading: "agent-chat-older-loading",
  /** Hidden mirror of the transcript viewport — `data-pinned`, `data-scroll-top`,
   *  `data-scroll-height`, `data-client-height`. The declared source of truth for scroll assertions
   *  (mirroring `terminal-page-scrollbar`), so a style change cannot silently turn a scroll test
   *  green by making the container unscrollable. */
  agentChatScrollState: "agent-chat-scroll-state",
  // PR-Stack Chat Screen — manually adding a planned PR (deterministic, non-chat path)
  prStackAddPlannedPrBtn: "pr-stack-add-planned-pr-btn",
  prStackAddPlannedPrForm: "pr-stack-add-planned-pr-form",
  prStackAddPlannedPrTitleInput: "pr-stack-add-planned-pr-title-input",
  prStackAddPlannedPrDescriptionInput: "pr-stack-add-planned-pr-description-input",
  prStackAddPlannedPrBranchSuggestionInput: "pr-stack-add-planned-pr-branch-suggestion-input",
  prStackAddPlannedPrSubmitBtn: "pr-stack-add-planned-pr-submit-btn",
  /**
   * Adds the planned PR and immediately opens the Start-session dialog for it, instead of leaving
   * the operator to find the new row and click its own CTA.
   */
  prStackAddPlannedPrStartBtn: "pr-stack-add-planned-pr-start-btn",
  prStackAddPlannedPrCancelBtn: "pr-stack-add-planned-pr-cancel-btn",
  prStackAddPlannedPrError: "pr-stack-add-planned-pr-error",
  // PR-Stack Chat Screen — the prompt shown when a pull targets a worktree with uncommitted work.
  // A dirty worktree is neither silently merged into nor a dead end: the operator is told what is
  // outstanding and can commit and push it before the pull proceeds.
  prStackDirtyWorktreeDialog: "pr-stack-dirty-worktree-dialog",
  prStackDirtyWorktreePaths: "pr-stack-dirty-worktree-paths",
  prStackDirtyWorktreeCommitMessageInput: "pr-stack-dirty-worktree-commit-message-input",
  prStackDirtyWorktreeCommitBtn: "pr-stack-dirty-worktree-commit-btn",
  prStackDirtyWorktreeCancelBtn: "pr-stack-dirty-worktree-cancel-btn",
  // session-drawer (#862): moved with the drawer-driven specs
  terminalDropRefused: "terminal-drop-refused",
  terminalUploadButton: "terminal-upload-button",
  uploadProgressIndicator: "upload-progress-indicator",
  uploadProgressError: "upload-progress-error",
} as const;

// ---------------------------------------------------------------------------
// Session inspector, agents and code pane (#861)
// ---------------------------------------------------------------------------

/** `[data-testid="agent-activity-row-<callId>"]` — one one-line record row in the activity overlay */
export const agentActivityRow = (callId: string) => `agent-activity-row-${callId}`;

/** `[data-testid="agent-conversation-turn-<index>"]` — one turn in the conversation transcript, in
 *  arrival order. Carries `data-role` (`operator` | `agent`), `data-complete`, and — on a finished
 *  agent turn — `data-stop-reason`. */
export const agentConversationTurn = (index: number) => `agent-conversation-turn-${index}`;

/** One error row for a host that could not answer the picker's fan-out. */
export const agentRosterPickerHostError = (daemonInstanceId: string) =>
  `agent-roster-picker-host-error-${safeTestIdPart(daemonInstanceId)}`;

/** One selectable agent in the fanned-out picker. */
export const agentRosterPickerOption = (agentId: string) =>
  `agent-roster-picker-option-${safeTestIdPart(agentId)}`;

/** The owning-daemon label on a picker option. */
export const agentRosterPickerOptionHost = (agentId: string) =>
  `${agentRosterPickerOption(agentId)}-host`;

/** The coding-agent badge on a picker option; absent on a model option. */
export const agentRosterPickerOptionKind = (agentId: string) =>
  `${agentRosterPickerOption(agentId)}-kind`;

/** One attached agent's row in the roster pane. */
export const agentRosterRow = (agentId: string) => `agent-roster-row-${safeTestIdPart(agentId)}`;

/** An attached agent's clone state; carries `data-clone-state`. */
export const agentRosterRowCloneState = (agentId: string) =>
  `${agentRosterRow(agentId)}-clone-state`;

/** The detach action on an attached agent's row. */
export const agentRosterRowDetachBtn = (agentId: string) =>
  `${agentRosterRow(agentId)}-detach-btn`;

/** The owning-daemon label on an attached agent's row. */
export const agentRosterRowHost = (agentId: string) => `${agentRosterRow(agentId)}-host`;

/** The coding-agent badge on a roster row; absent on a model row. */
export const agentRosterRowKind = (agentId: string) => `${agentRosterRow(agentId)}-kind`;

/** The agent's last observed activity. Absent when nothing has been observed of it. */
export const agentRosterRowLastActivity = (agentId: string) =>
  `${agentRosterRow(agentId)}-last-activity`;

/** The tools an attached agent withdraws from the main agent. */
export const agentRosterRowReplaces = (agentId: string) => `${agentRosterRow(agentId)}-replaces`;

/**
 * What the agent is doing (`data-agent-status`: unknown | idle | running | executing-tool |
 * waiting-for-input | connecting | error). Always present on a row — `unknown` is a value, not an
 * absence.
 */
export const agentRosterRowStatus = (agentId: string) => `${agentRosterRow(agentId)}-status`;

/** One subagent session's row in the tree. */
export const agentTreeSession = (sessionId: string) =>
  `agent-tree-session-${safeTestIdPart(sessionId)}`;

/** The list holding a subagent session's children — its roster agents and its own subagents. */
export const agentTreeSessionChildren = (sessionId: string) =>
  `${agentTreeSession(sessionId)}-children`;

/** A subagent session's last observed activity. Absent when nothing has been observed of it. */
export const agentTreeSessionLastActivity = (sessionId: string) =>
  `${agentTreeSession(sessionId)}-last-activity`;

/**
 * Why an expanded subagent session's roster could not be read. Absent while it reads fine — and it
 * is not the same thing as the node having no roster agents, which is what makes it worth a row of
 * its own.
 */
export const agentTreeSessionRosterError = (sessionId: string) =>
  `${agentTreeSession(sessionId)}-roster-error`;

/**
 * What a subagent session's agent is doing, inferred by the daemon from that session's own
 * conversation (`SessionEntry.agent_status`). Carries the same `data-agent-status` tokens a roster
 * row does, because it is the same enum.
 */
export const agentTreeSessionStatus = (sessionId: string) =>
  `${agentTreeSession(sessionId)}-status`;

/** Focuses a subagent session's runtime. A subagent session has no detach — there is no roster
 *  entry behind it. */
export const agentTreeSessionSwitchBtn = (sessionId: string) =>
  `${agentTreeSession(sessionId)}-switch-btn`;

/** Expands or collapses a subagent session's own children. Collapsed nodes hold no roster stream. */
export const agentTreeSessionToggleBtn = (sessionId: string) =>
  `${agentTreeSession(sessionId)}-toggle-btn`;

/**
 * `[data-testid="restructure-plan-row-<opId>-<column>"]` — one cell of an operation's row: `id`,
 * `op`, `item`, `group`, `status` or `stale`.
 */
export const restructurePlanCell = (opId: string, column: string) =>
  `restructure-plan-row-${opId}-${column}`;

/** `[data-testid="restructure-plan-row-<opId>"]` — one operation's row in the plan dialog. */
export const restructurePlanRow = (opId: string) => `restructure-plan-row-${opId}`;

/** One selectable agent in the header's picker. */
export const sessionAgentPickerOption = (agentId: string) =>
  `session-agent-picker-option-${safeTestIdPart(agentId)}`;

/** The Copy-host-path button of an uploaded-file row. */
export const sessionUploadCopyPath = (fileName: string) => `session-upload-copy-path-${fileName}`;

/** The Delete button (first step) of an uploaded-file row. */
export const sessionUploadDelete = (fileName: string) => `session-upload-delete-${fileName}`;

/** The Confirm-delete button (second step) of an uploaded-file row. */
export const sessionUploadDeleteConfirm = (fileName: string) =>
  `session-upload-delete-confirm-${fileName}`;

/** The Insert-into-terminal button of an uploaded-file row. */
export const sessionUploadInsert = (fileName: string) => `session-upload-insert-${fileName}`;

/** `[data-testid="session-upload-row-<fileName>"]` — one uploaded-file row. */
export const sessionUploadRow = (fileName: string) => `session-upload-row-${fileName}`;

/** The size readout inside an uploaded-file row. */
export const sessionUploadSize = (fileName: string) => `session-upload-size-${fileName}`;

/** `[data-testid="sessions-agent-pane-<conversationId>"]` — the mounted body of one agent
 *  conversation tab, shown when its tab is selected. */
export const sessionsAgentPane = (conversationId: string) =>
  `sessions-agent-pane-${safeTestIdPart(conversationId)}`;

/** `[data-testid="sessions-agent-tab-<conversationId>"]` — a tab for an open conversation with an
 *  agent attached to this session. Keyed by the conversation id rather than the agent id: the
 *  conversation is what the tab owns, and it is what closing the tab cancels. */
export const sessionsAgentTab = (conversationId: string) =>
  `sessions-agent-tab-${safeTestIdPart(conversationId)}`;

/** `[data-testid="sessions-agent-tab-<conversationId>-close"]` — the ✕ on an agent tab. Closing it
 *  cancels the conversation. Suffixed rather than infixed so the tab-listing selectors can exclude
 *  the close controls with a `$=` match. */
export const sessionsAgentTabClose = (conversationId: string) =>
  `${sessionsAgentTab(conversationId)}-close`;

/** `[data-testid="sessions-child-pane-<sessionId>"]` — the mounted runtime pane for a spawned
 *  child conversation, shown when its tab is selected. */
export const sessionsChildPane = (sessionId: string) => `sessions-child-pane-${sessionId}`;

/** `[data-testid="sessions-child-tab-<sessionId>"]` — a tab for a spawned child conversation
 *  (a child session whose `orchestratorSessionId` is the parent runtime's session). */
export const sessionsChildTab = (sessionId: string) => `sessions-child-tab-${sessionId}`;

/** `[data-testid="sessions-terminal-pane-<terminalId>"]` — the mounted terminal container for one
 *  terminal_id (active is CSS-visible, the rest are `display:none` but stay mounted). The Agent tab
 *  uses `terminalId = "main"`. */
export const sessionsTerminalPane = (terminalId: string) =>
  `sessions-terminal-pane-${terminalId}`;

/** `[data-testid="sessions-terminal-pane-stack-<sessionId>"]` — one runtime's stack of mounted
 *  terminal panes. This is the element handed to the Fullscreen API by the tab strip's ⛶ control. */
export const sessionsTerminalPaneStack = (sessionId: string) =>
  `sessions-terminal-pane-stack-${sessionId}`;

/** `[data-testid="sessions-terminal-tab-<terminalId>"]` — a single bash terminal tab. */
export const sessionsTerminalTab = (terminalId: string) =>
  `sessions-terminal-tab-${terminalId}`;

/** `[data-testid="sessions-terminal-tab-close-<terminalId>"]` — the ✕ close control on a bash tab. */
export const sessionsTerminalTabClose = (terminalId: string) =>
  `sessions-terminal-tab-close-${terminalId}`;

/** `[data-testid="sessions-terminal-tab-new-<kind>"]` — one item of the "+" menu: `bash`, or the
 *  session type of the CLI harness the new terminal runs (`warp-cli`, …). */
export const sessionsTerminalTabNewOption = (kind: string) =>
  `sessions-terminal-tab-new-${kind}`;

/**
 * `[data-testid="worktree-code-identifier-<line>-<column>"]` — the identifier token starting at a
 * one-based line and one-based byte column of the code preview.
 */
export const worktreeCodeIdentifier = (line: number, column: number) =>
  `worktree-code-identifier-${line}-${column}`;

/** `[data-testid="worktree-code-line-<line>"]` — one one-based line of the code preview. */
export const worktreeCodeLine = (line: number) => `worktree-code-line-${line}`;

/** `[data-testid="worktree-code-reference-<relPath>-<line>"]` — one entry of the references list. */
export const worktreeCodeReference = (relPath: string, line: number) =>
  `worktree-code-reference-${relPath}-${line}`;

/** `[data-testid="worktree-tree-node-<relPath>"]` — a single file/dir node in the worktree tree. */
export const worktreeTreeNode = (relPath: string) => `worktree-tree-node-${relPath}`;


// session-drawer (#862): selectors moved with the drawer-driven specs
/** `[data-testid="session-row-select-<sessionId>"]` */
export const sessionRowSelect = (sessionId: string) => `session-row-select-${sessionId}`;

/** `[data-testid="sessions-drawer-item-<sessionId>"]` — clickable drawer row */
export const sessionsDrawerItem = (sessionId: string) => `sessions-drawer-item-${sessionId}`;

/** `[data-testid="sessions-drawer-project-group-<projectId|other>"]` — one project's group in the
 *  expanded drawer (`#project-home` 2/7); `other` is the trailing group of project-less sessions. */
export const sessionsDrawerProjectGroup = (projectKey: string) =>
  `sessions-drawer-project-group-${projectKey}`;

/** `[data-testid="sessions-drawer-project-group-header-<projectId|other>"]` — the group's header:
 *  project name and session count; clicking it collapses or expands the group. */
export const sessionsDrawerProjectGroupHeader = (projectKey: string) =>
  `sessions-drawer-project-group-header-${projectKey}`;

/** `[data-testid="sessions-drawer-project-home-<projectId>"]` — the pinned **main** entry first in a
 *  project group; clicking it ensures and selects the project-home session (`#project-home` 4/7). */
export const sessionsDrawerProjectHome = (projectId: string) =>
  `sessions-drawer-project-home-${projectId}`;

/** `[data-testid="sessions-project-home-setup-<projectId>"]` — the detail pane a click on a pinned
 *  **main** entry opens when the project has no main (`/sessions/project-home/:projectId`), or its
 *  **Replace main** action opens (`…/replace`). */
export const sessionsProjectHomeSetup = (projectId: string) =>
  `sessions-project-home-setup-${projectId}`;

/** `[data-testid="sessions-project-home-setup-unknown-<projectId>"]` — shown when the host does not
 *  list the project the pane names. */
export const sessionsProjectHomeSetupUnknown = (projectId: string) =>
  `sessions-project-home-setup-unknown-${projectId}`;

/** `[data-testid="sessions-project-home-setup-unreachable-<projectId>"]` — shown instead of the form
 *  when no wire reaches the host main would run on. */
export const sessionsProjectHomeSetupUnreachable = (projectId: string) =>
  `sessions-project-home-setup-unreachable-${projectId}`;

/** `[data-testid="sessions-drawer-project-home-replace-<projectId>"]` — the pinned **main** entry's
 *  **Replace main** action, offered once main is set up; it opens the setup form in replace mode. */
export const sessionsDrawerProjectHomeReplace = (projectId: string) =>
  `sessions-drawer-project-home-replace-${projectId}`;

/** `[data-testid="sessions-project-home-setup-replacing-<projectId>"]` — the replace-mode notice in
 *  the setup pane: the current main is stopped and stays listed as an ordinary session. */
export const sessionsProjectHomeSetupReplacing = (projectId: string) =>
  `sessions-project-home-setup-replacing-${projectId}`;

/** `[data-testid="create-session-type-reason-<sessionType>"]` — why a session type the form shows is
 *  disabled (the main variant: every type but Claude CLI). */
export const createSessionTypeReason = (sessionType: string) =>
  `create-session-type-reason-${sessionType}`;

/** `[data-testid="project-main-session-link-<projectId>"]` — on a Projects card: **Set up main** when
 *  the project has no main, **Open main** when it has one. */
export const projectMainSessionLink = (projectId: string) =>
  `project-main-session-link-${projectId}`;

/** `[data-testid="sessions-drawer-stack-<parentSessionId>"]` — collapsible <details> group */
export const sessionsDrawerStackGroup = (parentSessionId: string) =>
  `sessions-drawer-stack-${parentSessionId}`;

/** `[data-testid="sessions-drawer-item-label-<sessionId>"]` — derived label text */
export const sessionsDrawerItemLabel = (sessionId: string) =>
  `sessions-drawer-item-label-${sessionId}`;

/** `[data-testid="sessions-drawer-item-status-<sessionId>"]` — connected/disconnected dot */
export const sessionsDrawerItemStatus = (sessionId: string) =>
  `sessions-drawer-item-status-${sessionId}`;

/** `[data-testid="sessions-drawer-item-tooltip-<sessionId>"]` — tooltip content showing full id */
export const sessionsDrawerItemTooltip = (sessionId: string) =>
  `sessions-drawer-item-tooltip-${sessionId}`;

/**
 * `[data-testid="sessions-drawer-item-orchestrator-<sessionId>"]` — the *orchestrator* marker on a row
 * whose session holds the `orchestrator` role (`#orchestrators` 1/11). Absent on every other row.
 */
export const sessionsDrawerItemOrchestrator = (sessionId: string) =>
  `sessions-drawer-item-orchestrator-${sessionId}`;

/** `[data-testid="sessions-drawer-item-host-<sessionId>"]` — owning-host badge on cross-host rows */
export const sessionsDrawerItemHost = (sessionId: string) =>
  `sessions-drawer-item-host-${sessionId}`;

/**
 * `[data-testid="sessions-drawer-item-codebase-host-<sessionId>"]` — codebase-host badge on a split
 * session, whose worktree lives on a different daemon than the one running its agent. Absent for
 * co-located sessions. See docs/ft/daemon/remote-managed-worktree.md.
 */
export const sessionsDrawerItemCodebaseHost = (sessionId: string) =>
  `sessions-drawer-item-codebase-host-${sessionId}`;

/** `[data-testid="sessions-detail-resume-<sessionId>"]` — Resume button in detail pane */
export const sessionsDetailResumeBtn = (sessionId: string) =>
  `sessions-detail-resume-${sessionId}`;

/** `[data-testid="sessions-detail-delete-<sessionId>"]` — Delete button in detail pane */
export const sessionsDetailDeleteBtn = (sessionId: string) =>
  `sessions-detail-delete-${sessionId}`;

/** `[data-testid="sessions-main-resume-<sessionId>"]` — Resume button in the main pane's top bar,
 *  present for every inactive session regardless of which base view is below it. */
export const sessionsMainResumeBtn = (sessionId: string) =>
  `sessions-main-resume-${sessionId}`;

/** `[data-testid="sessions-inspector-resume-<sessionId>"]` — Resume button in inspector */
export const sessionsInspectorResumeBtn = (sessionId: string) =>
  `sessions-inspector-resume-${sessionId}`;

/** `[data-testid="sessions-inspector-delete-<sessionId>"]` — Delete button in inspector */
export const sessionsInspectorDeleteBtn = (sessionId: string) =>
  `sessions-inspector-delete-${sessionId}`;

/** `[data-testid="sessions-inspector-delete-confirm-<sessionId>"]` — Delete confirm button */
export const sessionsInspectorDeleteConfirm = (sessionId: string) =>
  `sessions-inspector-delete-confirm-${sessionId}`;

/** `[data-testid="sessions-inspector-terminate-<sessionId>"]` — Terminate button in inspector */
export const sessionsInspectorTerminateBtn = (sessionId: string) =>
  `sessions-inspector-terminate-${sessionId}`;

/** `[data-testid="sessions-runtime-terminal-<sessionId>"]` — per-session terminal container
 *  mounted in the runtime registry (one per attached session; the focused one is CSS-visible,
 *  the others are `display:none` but still mounted). */
export const sessionsRuntimeTerminal = (sessionId: string) =>
  `sessions-runtime-terminal-${sessionId}`;

/** `[data-testid="sessions-child-attach-error-<sessionId>"]` — why a spawned child's attach failed. */
export const sessionsChildAttachError = (sessionId: string) => `sessions-child-attach-error-${sessionId}`;

/** `[data-testid="sessions-drawer-bulk-delete-error-<sessionId>"]` — why one bulk-deleted row survived. */
export const sessionsBulkDeleteError = (sessionId: string) =>
  `sessions-drawer-bulk-delete-error-${sessionId}`;

/** `[data-testid="sessions-vnc-target-row-<targetId>"]` — a single VNC target row */
export const sessionsVncTargetRow = (targetId: string) =>
  `sessions-vnc-target-row-${targetId}`;

/** `[data-testid="sessions-vnc-start-<targetId>"]` — Start stream button */
export const sessionsVncStartBtn = (targetId: string) => `sessions-vnc-start-${targetId}`;

/** `[data-testid="sessions-vnc-stop-<targetId>"]` — Stop stream button */
export const sessionsVncStopBtn = (targetId: string) => `sessions-vnc-stop-${targetId}`;

/** `[data-testid="sessions-vnc-remove-<targetId>"]` — Remove target button */
export const sessionsVncRemoveBtn = (targetId: string) => `sessions-vnc-remove-${targetId}`;

/** `[data-testid="sessions-usage-row-<id>"]` — a single conversation row */
export const sessionsUsageRow = (id: string) => `sessions-usage-row-${id}`;

/** `[data-testid="sessions-usage-row-agent-<id>"]` — the agent cell of a conversation row */
export const sessionsUsageRowAgent = (id: string) => `sessions-usage-row-agent-${id}`;

/** `[data-testid="sessions-usage-row-model-<id>"]` — the model cell of a conversation row */
export const sessionsUsageRowModel = (id: string) => `sessions-usage-row-model-${id}`;

/** `[data-testid="sessions-usage-row-input-<id>"]` — the input-tokens cell of a conversation row */
export const sessionsUsageRowInput = (id: string) => `sessions-usage-row-input-${id}`;
export const sessionsUsageRowCacheRead = (id: string) => `sessions-usage-row-cache-read-${id}`;
export const sessionsUsageRowCacheWrite = (id: string) => `sessions-usage-row-cache-write-${id}`;
export const sessionsUsageRowReasoning = (id: string) => `sessions-usage-row-reasoning-${id}`;
export const sessionsUsageRowCost = (id: string) => `sessions-usage-row-cost-${id}`;

/** `[data-testid="sessions-usage-row-output-<id>"]` — the output-tokens cell of a conversation row */
export const sessionsUsageRowOutput = (id: string) => `sessions-usage-row-output-${id}`;

/** `[data-testid="sessions-usage-row-total-<id>"]` — the total-tokens cell of a conversation row */
export const sessionsUsageRowTotal = (id: string) => `sessions-usage-row-total-${id}`;

/** `[data-testid="sessions-usage-row-turns-<id>"]` — the turns cell of a conversation row */
export const sessionsUsageRowTurns = (id: string) => `sessions-usage-row-turns-${id}`;

/** `[data-testid="sessions-screen-sharing-target-row-<targetId>"]` */
export const sessionsScreenSharingTargetRow = (targetId: string) =>
  `sessions-screen-sharing-target-row-${targetId}`;

/** `[data-testid="sessions-screen-sharing-start-<targetId>"]` — Start stream button */
export const sessionsScreenSharingStartBtn = (targetId: string) =>
  `sessions-screen-sharing-start-${targetId}`;

/** `[data-testid="sessions-screen-sharing-start-error-<targetId>"]` — why that target's start failed */
export const sessionsScreenSharingStartError = (targetId: string) =>
  `sessions-screen-sharing-start-error-${targetId}`;

/** `[data-testid="sessions-screen-sharing-stop-<targetId>"]` — Stop stream button */
export const sessionsScreenSharingStopBtn = (targetId: string) =>
  `sessions-screen-sharing-stop-${targetId}`;

/** `[data-testid="sessions-screen-sharing-remove-<targetId>"]` — Remove target button */
export const sessionsScreenSharingRemoveBtn = (targetId: string) =>
  `sessions-screen-sharing-remove-${targetId}`;

/** `[data-testid="pr-stack-planned-pr-row-<nodeId>"]` — a single planned-PR row */
export const prStackPlannedPrRow = (nodeId: string) => `pr-stack-planned-pr-row-${nodeId}`;

/** `[data-testid="pr-stack-start-session-<nodeId>"]` — "Start session" CTA for an unspawned node */
export const prStackStartSessionBtn = (nodeId: string) => `pr-stack-start-session-${nodeId}`;

/** `[data-testid="pr-stack-status-chip-<nodeId>"]` — status chip for an already-spawned node */
export const prStackStatusChip = (nodeId: string) => `pr-stack-status-chip-${nodeId}`;

/** `[data-testid="pr-stack-internal-status-badge-<nodeId>"]` — the action-needed internal-status badge */
export const prStackInternalStatusBadge = (nodeId: string) =>
  `pr-stack-internal-status-badge-${nodeId}`;

/** `[data-testid="pr-stack-in-progress-<nodeId>"]` — in-progress indicator (a live session owns the branch) */
export const prStackInProgressBadge = (nodeId: string) => `pr-stack-in-progress-${nodeId}`;

/** `[data-testid="pr-stack-pr-link-<nodeId>"]` — the GitHub PR number rendered as a link to the PR */
export const prStackPrLink = (nodeId: string) => `pr-stack-pr-link-${nodeId}`;

/** `[data-testid="pr-stack-pr-state-<nodeId>"]` — the GitHub PR state (open/merged/closed/draft) */
export const prStackPrState = (nodeId: string) => `pr-stack-pr-state-${nodeId}`;

/** `[data-testid="pr-stack-repoint-<nodeId>"]` — the Repoint control shown when a predecessor merged */
export const prStackRepointBtn = (nodeId: string) => `pr-stack-repoint-${nodeId}`;

/** `[data-testid="pr-stack-worktree-<nodeId>"]` — the on-disk worktree indicator (from QueryBranch) */
export const prStackWorktree = (nodeId: string) => `pr-stack-worktree-${nodeId}`;

/** `[data-testid="pr-stack-session-<nodeId>"]` — the control that opens the node's bound child
 *  session, wrapping the status chip. Present only when the node's recorded child session — or,
 *  failing that, the session that owns its branch — resolves to a session the drawer knows */
export const prStackSession = (nodeId: string) => `pr-stack-session-${nodeId}`;

/** `[data-testid="pr-stack-row-toggle-<nodeId>"]` — the row header that expands/collapses its detail */
export const prStackRowToggle = (nodeId: string) => `pr-stack-row-toggle-${nodeId}`;

/** `[data-testid="pr-stack-row-details-<nodeId>"]` — the row's detail body. Always mounted and hidden
 *  when collapsed, so expansion, scroll position and the branch poll set survive a collapse */
export const prStackRowDetails = (nodeId: string) => `pr-stack-row-details-${nodeId}`;

/** `[data-testid="pr-stack-node-id-<nodeId>"]` — the planner-assigned node id, in the row's detail */
export const prStackNodeId = (nodeId: string) => `pr-stack-node-id-${nodeId}`;

/** `[data-testid="pr-stack-parents-<nodeId>"]` — the titles of the nodes this one is stacked on */
export const prStackParents = (nodeId: string) => `pr-stack-parents-${nodeId}`;

/** `[data-testid="pr-stack-child-recipe-<nodeId>"]` — the recipe the node's child session runs */
export const prStackChildRecipe = (nodeId: string) => `pr-stack-child-recipe-${nodeId}`;

/** `[data-testid="pr-stack-child-session-<nodeId>"]` — the bound child session's id, in the detail */
export const prStackChildSession = (nodeId: string) => `pr-stack-child-session-${nodeId}`;

/** `[data-testid="pr-stack-child-state-<nodeId>"]` — the coarse mirror of the child session's
 *  workflow state the orchestrator last recorded. Stale whenever the orchestrator agent is not
 *  running, which is why it is a detail line rather than a badge */
export const prStackChildState = (nodeId: string) => `pr-stack-child-state-${nodeId}`;

/** `[data-testid="pr-stack-move-up-<nodeId>"]` — move the row one position earlier in the persisted order */
export const prStackMoveUpBtn = (nodeId: string) => `pr-stack-move-up-${nodeId}`;

/** `[data-testid="pr-stack-move-down-<nodeId>"]` — move the row one position later in the persisted order */
export const prStackMoveDownBtn = (nodeId: string) => `pr-stack-move-down-${nodeId}`;

/** `[data-testid="pr-stack-reorder-error-<nodeId>"]` — the daemon's reason for refusing or failing a
 *  reorder. Rendered outside the row's collapse boundary: a refused reorder moves nothing, which is
 *  indistinguishable from a swallowed click unless the row says why */
export const prStackReorderError = (nodeId: string) => `pr-stack-reorder-error-${nodeId}`;

/** `[data-testid="pr-stack-base-behind-<nodeId>"]` — how many commits the branch is behind its base */
export const prStackBaseBehind = (nodeId: string) => `pr-stack-base-behind-${nodeId}`;

/** `[data-testid="pr-stack-base-in-sync-<nodeId>"]` — the branch contains every commit on its base.
 *  Rendered rather than staying silent: without it a clean row and a row whose comparison has not
 *  arrived would look identical, which is the conflation the unavailable discriminator exists to stop */
export const prStackBaseInSync = (nodeId: string) => `pr-stack-base-in-sync-${nodeId}`;

/** `[data-testid="pr-stack-base-conflicts-<nodeId>"]` — merging the base into the branch would conflict */
export const prStackBaseConflicts = (nodeId: string) => `pr-stack-base-conflicts-${nodeId}`;

/** `[data-testid="pr-stack-base-sync-unavailable-<nodeId>"]` — the base comparison could not be made
 *  (no base named, a ref that resolves to nothing, a git failure). A failed comparison arrives
 *  byte-identical to a healthy one, so it must never render as "in sync" */
export const prStackBaseSyncUnavailable = (nodeId: string) => `pr-stack-base-sync-unavailable-${nodeId}`;

/** `[data-testid="pr-stack-base-conflict-paths-<nodeId>"]` — the conflicting paths, in the row's detail */
export const prStackBaseConflictPaths = (nodeId: string) => `pr-stack-base-conflict-paths-${nodeId}`;

/** `[data-testid="pr-stack-sync-merge-<nodeId>"]` — pull the base in by merging it (the default) */
export const prStackSyncMergeBtn = (nodeId: string) => `pr-stack-sync-merge-${nodeId}`;

/** `[data-testid="pr-stack-sync-rebase-<nodeId>"]` — pull the base in by rebasing onto it */
export const prStackSyncRebaseBtn = (nodeId: string) => `pr-stack-sync-rebase-${nodeId}`;

/** `[data-testid="pr-stack-sync-error-<nodeId>"]` — the daemon's reason for refusing or failing a pull.
 *  Rendered outside the row's collapse boundary: a reason the operator must expand a row to find is
 *  the dead end the always-visible warning region exists to remove */
export const prStackSyncError = (nodeId: string) => `pr-stack-sync-error-${nodeId}`;

/** `[data-testid="pr-stack-branch-<nodeId>"]` — the branch this planned PR owns (a branch that exists) */
export const prStackBranch = (nodeId: string) => `pr-stack-branch-${nodeId}`;

/** `[data-testid="pr-stack-planned-branch-<nodeId>"]` — the planned branch *name* (`branch_suggestion`),
 *  rendered distinctly from an owned branch because a suggestion names no ref yet */
export const prStackPlannedBranch = (nodeId: string) => `pr-stack-planned-branch-${nodeId}`;

/** `[data-testid="pr-stack-base-branch-<nodeId>"]` — the branch this planned PR's child worktree would
 *  be based onto (its nearest usable ancestor's branch, or the project default) */
export const prStackBaseBranch = (nodeId: string) => `pr-stack-base-branch-${nodeId}`;

/** `[data-testid="pr-stack-start-warning-<nodeId>"]` — the warning listing every reason the node cannot
 *  be started. The row's information and its Start-session CTA stay put beside it */
export const prStackStartWarning = (nodeId: string) => `pr-stack-start-warning-${nodeId}`;

/** `[data-testid="pr-stack-start-session-blocked-icon-<nodeId>"]` — the alert icon carried by a blocked
 *  node's Start-session button. The button stays pressable (a blocker is advice, not a refusal: three
 *  of `QueryBranch`'s legs cannot see another host), so the icon plus the warning colour is what tells
 *  the operator they are about to start something the view believes is blocked */
export const prStackStartSessionBlockedIcon = (nodeId: string) =>
  `pr-stack-start-session-blocked-icon-${nodeId}`;

/** `[data-testid="pr-stack-repoint-error-<nodeId>"]` — the daemon's reason for refusing a repoint. The
 *  RPC can reject (a stale target names no acceptable base), so failing silently would leave the row
 *  looking untouched with no explanation — the dead end this feature exists to remove */
export const prStackRepointError = (nodeId: string) => `pr-stack-repoint-error-${nodeId}`;

/** `[data-testid="pr-stack-pr-unavailable-<nodeId>"]` — shown when the GitHub PR lookup could not be
 *  performed (no credential, rate limit, transport error) — distinct from "this branch has no PR" */
export const prStackPrUnavailable = (nodeId: string) => `pr-stack-pr-unavailable-${nodeId}`;

/** `[data-testid="agent-chat-message-<index>"]` — a single rendered chat bubble (reusable AgentChat) */
export const agentChatMessage = (index: number) => `agent-chat-message-${index}`;

/** `[data-testid="agent-chat-option-<index>"]` — a single-select option button (reusable AgentChat) */
export const agentChatOption = (index: number) => `agent-chat-option-${index}`;

/** `[data-testid="agent-chat-multiselect-option-<index>"]` — a multi-select option checkbox (reusable AgentChat) */
export const agentChatMultiSelectOption = (index: number) => `agent-chat-multiselect-option-${index}`;

/** `[data-testid="agent-chat-elapsed-<index>"]` — the DEBUG-style "+Ns" elapsed badge on a
 *  read-only transcript entry (wall-clock since the previous entry). */
export const agentChatElapsed = (index: number) => `agent-chat-elapsed-${index}`;

/** `[data-testid="agent-chat-tool-status-<index>"]` — the status marker (running/error) on a
 *  read-only transcript tool-call entry. */
export const agentChatToolStatus = (index: number) => `agent-chat-tool-status-${index}`;

/** `[data-testid="pr-stack-add-planned-pr-ancestor-<nodeId>"]` — an ancestor checkbox in the "New planned PR" form */
export const prStackAddPlannedPrAncestorCheckbox = (nodeId: string) =>
  `pr-stack-add-planned-pr-ancestor-${nodeId}`;

/** `[data-testid="cpu-core-bar-<index>"]` — the mini bar for logical core `<index>` (0-based). */
export const cpuCoreBar = (index: number) => `cpu-core-bar-${index}`;

/** `[data-testid="session-upload-delete-error-<fileName>"]` — why deleting that upload failed */
export const sessionUploadDeleteError = (fileName: string) =>
  `session-upload-delete-error-${fileName}`;

/** `[data-testid="sessions-vnc-start-error-<targetId>"]` — why that VNC target's start failed */
export const sessionsVncStartError = (targetId: string) => `sessions-vnc-start-error-${targetId}`;

/** `[data-testid="sessions-vnc-remove-error-<targetId>"]` — why removing that VNC target failed */
export const sessionsVncRemoveError = (targetId: string) => `sessions-vnc-remove-error-${targetId}`;

/** `[data-testid="sessions-screen-sharing-remove-error-<targetId>"]` — why removing it failed */
export const sessionsScreenSharingRemoveError = (targetId: string) =>
  `sessions-screen-sharing-remove-error-${targetId}`;

/** An attachment row, keyed by the basename it will be materialized under. */
export const createSessionAttachmentRow = (basename: string) =>
  `create-session-attachment-row-${basename}`;

/** The editable basename field of an attachment row. Renaming changes only
 *  `SessionAttachment.basename`; the source locator (and the stored file) is untouched. */
export const createSessionAttachmentName = (basename: string) =>
  `create-session-attachment-name-${basename}`;

/** An attachment row's size, in bytes, as the host will see it. */
export const createSessionAttachmentSize = (basename: string) =>
  `create-session-attachment-size-${basename}`;

/** Removes an attachment row before the session is created. */
export const createSessionAttachmentRemove = (basename: string) =>
  `create-session-attachment-remove-${basename}`;

/** Per-row materialization progress, carrying `data-attachment-percent` so a mid-stream
 *  assertion reads an exact value rather than matching rendered text. */
export const createSessionAttachmentProgress = (basename: string) =>
  `create-session-attachment-progress-${basename}`;

/** One selectable document row in the host-document picker. */
export const createSessionHostDocRow = (relativePath: string) =>
  `create-session-host-doc-row-${relativePath}`;

/** A selectable agent in the create-session form's multi-select, keyed by qualified id. */
export const createSessionAgentOption = (agentId: string) =>
  `create-session-agent-${safeTestIdPart(agentId)}`;

/**
 * One `<option>` in the create-session form's **primary** Agent `<select>` — the agent a session is
 * started *as*, distinct from the specialized-agent multi-select above. Keyed by the option's own
 * value: `{id}@{daemonInstanceId}` while the common room advertises daemons, the bare id otherwise.
 */
export const createSessionAgentSelectOption = (optionValue: string) =>
  `create-session-agent-select-option-${safeTestIdPart(optionValue)}`;

/** One error row for a host whose agent catalog (`ListAgents`) could not be read. */
export const createSessionAgentSelectHostError = (daemonInstanceId: string) =>
  `create-session-agent-select-host-error-${safeTestIdPart(daemonInstanceId)}`;
