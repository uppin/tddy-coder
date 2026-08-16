/**
 * Centralised data-testid constants.
 *
 * Every `data-testid` value used by the Cypress suite lives here — the raw string
 * appears once; tests use the named constant so a rename is a one-line change.
 *
 * For identifiers that include a dynamic segment (session ID, project ID, …) the
 * constant is a prefix or a helper function — see the examples below.
 */

import { safeTestIdPart } from "../../src/lib/testId";

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
  // Auth
  githubLoginButton: "github-login-button",
  userLogin: "user-login",

  // App / Connection
  livekitUrl: "livekit-url",
  livekitRoom: "livekit-room",
  livekitIdentity: "livekit-identity",
  livekitStatus: "livekit-status",
  buildId: "build-id",

  // Terminal chrome
  connectionStatusDot: "connection-status-dot",
  connectionMenuDisconnect: "connection-menu-disconnect",
  connectionMenuTerminate: "connection-menu-terminate",
  connectedTerminalContainer: "connected-terminal-container",
  terminalReconnectOverlayRoot: "terminal-reconnect-overlay-root",
  terminalReconnectExpand: "terminal-reconnect-expand",
  connectionError: "connection-error",

  // Terminal
  ghosttyTerminal: "ghostty-terminal",
  terminalFullscreenButton: "terminal-fullscreen-button",
  terminalConnectionStatusBar: "terminal-connection-status-bar",
  mobileKeyboardButton: "mobile-keyboard-button",
  ctrlCButton: "ctrl-c-button",
  // File drop-to-upload (docs/ft/web/web-terminal.md § File drop upload)
  terminalDropOverlay: "terminal-drop-overlay",
  terminalUploadButton: "terminal-upload-button",
  uploadProgressIndicator: "upload-progress-indicator",
  uploadProgressError: "upload-progress-error",
  // Lazy scroll-up history (docs/ft/web/terminal-replay-lazy-scroll.md)
  loadEarlierHistory: "load-earlier-history",
  /** "View history" affordance — shown on the live pane after the first fill, swaps to the page pane. */
  viewHistory: "view-history",
  /** "Back to live" affordance — shown on the page pane, swaps back to the live pane. */
  backToLive: "back-to-live",
  /** Loading indicator shown while the background page terminal is being forward-filled. */
  terminalHistoryLoading: "terminal-history-loading",
  /** The overlay layer holding the live (scrollback=0, streaming) terminal. */
  terminalLivePane: "terminal-live-pane",
  /** The overlay layer holding the older-history (scrollback>0) page terminal. */
  terminalPagePane: "terminal-page-pane",
  terminalOlderBufferText: "terminal-older-buffer-text",
  /** Hidden mirror of the page terminal's viewportY (lines scrolled up from the bottom). */
  terminalPageViewportY: "terminal-page-viewport-y",
  /** Hidden mirror of the LIVE terminal's viewportY (lines scrolled up from the bottom). */
  terminalLiveViewportY: "terminal-live-viewport-y",
  /** Hidden mirror of the LIVE terminal's scrollback length (history lines, excluding active screen). */
  terminalLiveScrollbackLength: "terminal-live-scrollback-length",
  /** Hidden mirror of the page terminal's native Scrollbar {total,offset,len} — "total,offset,len". */
  terminalPageScrollbar: "terminal-page-scrollbar",
  /** Hidden mirror of the LIVE terminal's native Scrollbar {total,offset,len} — "total,offset,len". */
  terminalLiveScrollbar: "terminal-live-scrollbar",
  // Enqueued input overlay (docs/ft/web/enqueued-input-overlay.md)
  enqueuedInputOverlay: "enqueued-input-overlay",
  enqueuedInputText: "enqueued-input-text",
  enqueuedInputMouse: "enqueued-input-mouse",
  enqueuedInputOverflow: "enqueued-input-overflow",

  // ConnectionScreen / session table
  sessionsTableOrphan: "sessions-table-orphan",

  // Participants
  participantList: "participant-list",
  participantListEmpty: "participant-list-empty",
  participantListError: "participant-list-error",
  connectedParticipantsPanel: "connected-participants-panel",

  // LiveKit rooms panel
  livekitRoomsPanel: "livekit-rooms-panel",
  livekitRoomsPanelLoading: "livekit-rooms-panel-loading",
  livekitRoomsPanelEmpty: "livekit-rooms-panel-empty",
  livekitRoomsPanelError: "livekit-rooms-panel-error",

  // Worktrees
  shellMenuWorktrees: "shell-menu-worktrees",
  worktreesScreen: "worktrees-screen",
  worktreesProjectSelect: "worktrees-project-select",
  worktreesTable: "worktrees-table",
  worktreeRow: "worktrees-row",
  worktreeDelete: "worktrees-delete",
  worktreeDeleteConfirm: "worktrees-delete-confirm",
  worktreeDeletedPath: "worktrees-deleted-path",
  // Lazy, streamed disk usage (docs/ft/web/worktree-disk-usage-streaming.md)
  worktreeStatus: "worktrees-status",
  worktreeLastCalculated: "worktrees-last-calculated",
  worktreeCalculate: "worktrees-calculate",
  worktreesRecalculateAll: "worktrees-recalculate-all",
  worktreesCalculatedPath: "worktrees-calculated-path",
  worktreesRecalculatedAll: "worktrees-recalculated-all",

  // Session Inspector → Worktree tab (docs/ft/web/session-worktree-inspector.md)
  sessionInspectorTabWorktree: "sessions-inspector-tab-worktree",
  sessionWorktreeTab: "session-worktree-tab",
  sessionWorktreeSize: "session-worktree-size",
  sessionWorktreeLastCalculated: "session-worktree-last-calculated",
  sessionWorktreeBranch: "session-worktree-branch",
  sessionWorktreeChanged: "session-worktree-changed",
  sessionWorktreeRefresh: "session-worktree-refresh",
  sessionWorktreeClear: "session-worktree-clear",
  sessionWorktreeClearConfirm: "session-worktree-clear-confirm",
  sessionWorktreeDelete: "session-worktree-delete",
  sessionWorktreeDeleteConfirm: "session-worktree-delete-confirm",
  sessionWorktreeMissing: "session-worktree-missing",
  sessionWorktreeRestore: "session-worktree-restore",

  // Session Inspector → Files tab (docs/ft/web/session-files-inspector.md)
  sessionInspectorTabFiles: "sessions-inspector-tab-files",
  sessionFilesPanel: "session-files-panel",
  sessionFilesEmpty: "session-files-empty",

  // CodexOAuth dialog
  codexOauthDialog: "codex-oauth-dialog",
  codexOauthDismiss: "codex-oauth-dismiss",
  codexOauthEmbeddingFallback: "codex-oauth-embedding-fallback",

  // Visual viewport consumer (test harness)
  viewportConsumer: "viewport-consumer",
  viewportHeight: "viewport-height",
  viewportKeyboardOpen: "viewport-keyboard-open",

  // Session files panel
  sessionFilePreview: "session-file-preview",
  yamlSyntaxHighlight: "yaml-syntax-highlight",

  // Session more-actions menu
  sessionMoreActionsShowFiles: "session-more-actions-show-files",

  // Worktree Code pane (docs/ft/web/session-code-pane.md)
  worktreeCodeToggle: "sessions-code-toggle",
  worktreeCodePane: "worktree-code-pane",
  worktreeFileTree: "worktree-file-tree",
  worktreeFilePreview: "worktree-file-preview",
  worktreeCodeHighlight: "worktree-code-highlight",

  // Sessions drawer screen
  sessionsDrawerScreen: "sessions-drawer-screen",
  sessionsDrawer: "sessions-drawer",
  sessionsDetailPane: "sessions-detail-pane",
  sessionsDetailTerminalContainer: "sessions-detail-terminal-container",
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
  sessionsInspectorMetadata: "sessions-inspector-metadata",

  // Session inspector — tabs
  sessionsInspectorTabDetails: "sessions-inspector-tab-details",
  sessionsInspectorTabTools: "sessions-inspector-tab-tools",
  sessionsInspectorToolsPanel: "sessions-inspector-tools-panel",

  // Session inspector — Tools tab: invoke panel
  sessionsToolInvokeSelect: "sessions-tool-invoke-select",
  sessionsToolInvokeArgs: "sessions-tool-invoke-args",
  sessionsToolInvokeButton: "sessions-tool-invoke-button",
  sessionsToolInvokeResult: "sessions-tool-invoke-result",
  sessionsToolInvokeError: "sessions-tool-invoke-error",

  // Session inspector — Tools tab: call log
  sessionsToolCallLog: "sessions-tool-call-log",
  sessionsToolCallRow: "sessions-tool-call-row",
  sessionsToolCallInput: "sessions-tool-call-input",
  sessionsToolCallOutput: "sessions-tool-call-output",
  sessionsToolCallStdio: "sessions-tool-call-stdio",

  // Tasks drawer screen
  tasksDrawerScreen: "tasks-drawer-screen",
  tasksDrawer: "tasks-drawer",
  tasksOutputPane: "tasks-output-pane",
  tasksOutputPaneEmpty: "tasks-output-pane-empty",

  // Sessions drawer — bulk select + delete (bottom selection minibar)
  /** The selection minibar pinned to the bottom of the open drawer. */
  sessionsDrawerSelectBar: "sessions-drawer-select-bar",
  /** "Select" button that activates selection mode (reveals per-row checkboxes). */
  sessionsDrawerSelectMode: "sessions-drawer-select-mode",
  /** "Select all" / "Deselect all" toggle, shown while in selection mode. */
  sessionsDrawerSelectAll: "sessions-drawer-select-all",
  /** "Cancel" button that exits selection mode and clears the selection. */
  sessionsDrawerSelectCancel: "sessions-drawer-select-cancel",
  /** Single "delete selected" action button, enabled once ≥1 row checkbox is ticked. */
  sessionsDrawerBulkDelete: "sessions-drawer-bulk-delete",

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
  /** Host <select> — which daemon/host runs the session (multi-daemon). Rendered only when the
   *  common room advertises at least one daemon. */
  createSessionHostSelect: "create-session-host-select",
  /** Overlay dialog wrapping CreateSessionPane, used by the PR-stack "Start session" flow. */
  createSessionDialog: "create-session-dialog",
  createSessionTypeToolBtn: "create-session-type-tool",
  createSessionTypeClaudeCliBtn: "create-session-type-claude-cli",
  createSessionTypeCursorCliBtn: "create-session-type-cursor-cli",
  createSessionProjectSelect: "create-session-project-select",
  createSessionAgentSelect: "create-session-agent-select",
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
  /** Inline error shown when the model probe (ListAgentModels) fails for the selected agent. */
  createSessionModelError: "create-session-model-error",
  createSessionPermissionModeSelect: "create-session-permission-mode-select",
  /** Checkbox that sets `StartSessionRequest.dangerously_skip_permissions = true` (claude-cli only).
   * Mutually exclusive with the permission-mode select, which it disables while checked. */
  createSessionDangerouslySkipPermissionsToggle:
    "create-session-dangerously-skip-permissions-toggle",
  createSessionSandboxToggle: "create-session-sandbox-toggle",
  /** Collapsible "Managed codebase" section header — claude-cli sessions only. See
   * docs/ft/coder/specialized-subagents.md. */
  createSessionManagedCodebaseToggle: "create-session-managed-codebase-toggle",
  /** Expanded "Managed codebase" section content (specialized-subagent multi-select). */
  createSessionManagedCodebaseSection: "create-session-managed-codebase-section",
  /** "Semantic index" checkbox inside the Managed codebase section — when on, the daemon indexes the
   * worktree before launch and exposes the SemanticSearch tool. See docs/ft/coder/semantic-index.md. */
  createSessionSemanticIndexToggle: "create-session-semantic-index-toggle",
  /** "Codebase host" <select> inside the Managed codebase section — which daemon's filesystem holds
   * the session's git worktree. Empty value means "same as host" (co-located). claude-cli only.
   * See docs/ft/daemon/remote-managed-worktree.md. */
  createSessionCodebaseHostSelect: "create-session-codebase-host-select",
  createSessionInitialPromptInput: "create-session-initial-prompt-input",
  createSessionBranchIntentSelect: "create-session-branch-intent-select",
  createSessionNewBranchNameInput: "create-session-new-branch-name-input",
  createSessionBranchToWorkOnSelect: "create-session-branch-to-work-on-select",
  /** "Create Remote Branch" checkbox — pre-checked; pushes the new branch to origin at session start. */
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

  // Session agents — peer agent sessions section in SessionMainPane
  /** The "Add agent" button in the session-detail header (spawns a peer child session). */
  sessionAgentsAddBtn: "session-agents-add-btn",
  /** The collapsible "Session agents" section listing the current session's peers. */
  sessionAgentsSection: "session-agents-section",
  /** Empty-state message shown when the current session has no peers. */
  sessionAgentsEmpty: "session-agents-empty",
  /** One row per peer, keyed by the peer's session id. */
  sessionAgentsRow: "session-agents-row",
  /** The "switch" action on a peer row — focuses that peer's runtime. */
  sessionAgentsSwitchBtn: "session-agents-switch-btn",

  // Shell navigation
  shellMenuButton: "shell-menu-button",
  shellMenuSessions: "shell-menu-sessions",
  shellMenuLivekit: "shell-menu-livekit",
  shellMenuTasks: "shell-menu-tasks",
  shellMenuProjects: "shell-menu-projects",
  shellMenuVms: "shell-menu-vms",
  shellMenuModels: "shell-menu-models",
  shellMenuRpcPlayground: "shell-menu-rpc-playground",

  // Models & Agents screen (#/models)
  /** The Models & Agents screen root. */
  modelsScreen: "models-screen",
  /** The providers panel listing this fleet's configured providers. */
  modelsProvidersPanel: "models-providers-panel",
  /** Opens the add-provider form. */
  modelsAddProviderToggle: "models-add-provider-toggle",
  /** Add-provider form fields. */
  modelsAddProviderKind: "models-add-provider-kind",
  modelsAddProviderLabel: "models-add-provider-label",
  modelsAddProviderBaseUrl: "models-add-provider-base-url",
  modelsAddProviderApiKey: "models-add-provider-api-key",
  modelsAddProviderSubmit: "models-add-provider-submit",
  /** The error the add-provider form reports when the daemon refused (or was never addressed). */
  modelsAddProviderError: "models-add-provider-error",
  /** The models table listing every model across every connected daemon. */
  modelsTable: "models-table",
  /** The row the table shows instead of models; carries `data-registry-status` for why. */
  modelsTableEmpty: "models-table-empty",
  /** The assistants panel. */
  modelsAssistantsPanel: "models-assistants-panel",
  /** Opens the create-assistant dialog for the focused model. */
  modelsCreateAssistantDialog: "models-create-assistant-dialog",
  modelsCreateAssistantName: "models-create-assistant-name",
  modelsCreateAssistantLabel: "models-create-assistant-label",
  modelsCreateAssistantSystemPrompt: "models-create-assistant-system-prompt",
  modelsCreateAssistantSubmit: "models-create-assistant-submit",
  /** The ACP chat dialog opened from a model or assistant row. */
  modelsChatDialog: "models-chat-dialog",
  modelsChatInput: "models-chat-input",
  modelsChatSend: "models-chat-send",
  modelsChatTranscript: "models-chat-transcript",

  // RPC Playground
  rpcPlaygroundParticipantSelect: "rpc-playground-participant-select",
  rpcServiceTree: "rpc-service-tree",
  rpcRequestEditor: "rpc-request-editor",
  rpcInvokeButton: "rpc-invoke-button",
  rpcServiceTree: "rpc-service-tree",
  rpcRequestEditor: "rpc-request-editor",
  rpcRequestRawJson: "rpc-request-raw-json",
  rpcEditorToggleRaw: "rpc-editor-toggle-raw",
  rpcEditorToggleBuilder: "rpc-editor-toggle-builder",
  rpcInvokeButton: "rpc-invoke-button",
  rpcResponse: "rpc-response",
  rpcError: "rpc-error",

  // Terminal routes
  terminalRouteUnknownSession: "terminal-route-unknown-session",
  terminalRouteUnknownSessionHome: "terminal-route-unknown-session-home",

  // Shortcut drawer
  shortcutDrawer: "shortcut-drawer",
  shortcutDragHandle: "shortcut-drag-handle",

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

  // Session terminal tabs — Agent + bash terminals per session
  /** The terminal tab strip at the top of the session runtime area. */
  sessionsTerminalTabs: "sessions-terminal-tabs",
  /** The fixed, non-closable Agent tab (reserved "main" terminal). */
  sessionsTerminalTabAgent: "sessions-terminal-tab-agent",
  /** The "+" button that opens a new bash terminal. */
  sessionsTerminalTabNew: "sessions-terminal-tab-new",

  // Session connection overlay — covers the runtime's panes until LiveKit is connected
  sessionConnectionOverlay: "session-connection-overlay",
  sessionConnectionError: "session-connection-error",

  // Terminal control mutex — "Claim terminal" CTA
  terminalControlOverlay: "terminal-control-overlay",
  terminalClaimBtn: "terminal-claim-btn",
  terminalControlHolder: "terminal-control-holder",
  // Control-token reset regression harness (SessionRuntimeControlTokenReset.cy.tsx)
  controlTokenDisplay: "control-token-display",
  switchSession: "switch-session",
  // Steal-claim regression harness (SessionRuntimeStealClaimReattach.cy.tsx)
  controlIsControllerDisplay: "control-is-controller-display",
  controlHolderDisplay: "control-holder-display",

  // Session inspector — Usage tab
  sessionsInspectorTabUsage: "sessions-inspector-tab-usage",
  sessionsUsageTabPanel: "sessions-usage-tab-panel",
  sessionsUsageEmpty: "sessions-usage-empty",
  sessionsUsageTotalInput: "sessions-usage-total-input",
  sessionsUsageTotalOutput: "sessions-usage-total-output",
  sessionsUsageTotalTotal: "sessions-usage-total-total",

  // Session inspector — VNC tab
  sessionsInspectorTabVnc: "sessions-inspector-tab-vnc",
  sessionsVncTabPanel: "sessions-vnc-tab-panel",
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
  sessionsVncPassphraseCancel: "sessions-vnc-passphrase-cancel",

  // VNC overlay
  vncOverlay: "vnc-overlay",
  vncOverlayVideo: "vnc-overlay-video",
  vncOverlayClose: "vnc-overlay-close",

  // Session inspector — Screen Sharing tab
  sessionsInspectorTabScreenSharing: "sessions-inspector-tab-screen-sharing",
  sessionsScreenSharingTabPanel: "sessions-screen-sharing-tab-panel",
  sessionsScreenSharingTargetList: "sessions-screen-sharing-target-list",
  sessionsScreenSharingAddForm: "sessions-screen-sharing-add-form",
  sessionsScreenSharingAddLabel: "sessions-screen-sharing-add-label",
  sessionsScreenSharingAddHost: "sessions-screen-sharing-add-host",
  sessionsScreenSharingAddPort: "sessions-screen-sharing-add-port",
  sessionsScreenSharingAddPassword: "sessions-screen-sharing-add-password",
  sessionsScreenSharingAddProtocol: "sessions-screen-sharing-add-protocol",
  sessionsScreenSharingAddSubmit: "sessions-screen-sharing-add-submit",
  sessionsScreenSharingPassphraseDialog: "sessions-screen-sharing-passphrase-dialog",
  sessionsScreenSharingPassphraseInput: "sessions-screen-sharing-passphrase-input",
  sessionsScreenSharingPassphraseConfirm: "sessions-screen-sharing-passphrase-confirm",
  sessionsScreenSharingPassphraseCancel: "sessions-screen-sharing-passphrase-cancel",

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

  // Agent Activity pane (per-session, top-bar overlay of the agent's own tool calls)
  /** Top-bar icon button — rendered only when the session has ≥1 tool-call record. */
  agentActivityButton: "agent-activity-button",
  /** Unread-activity badge on the icon — shown while `unreadCount > 0`. */
  agentActivityUnreadBadge: "agent-activity-unread-badge",
  /** The in-pane overlay listing one-line records. */
  agentActivityOverlay: "agent-activity-overlay",
  /** Close control on the overlay. */
  agentActivityOverlayClose: "agent-activity-overlay-close",
  /** Tool-call detail dialog opened by clicking a tool entry in the transcript. */
  agentActivityDetailDialog: "agent-activity-detail-dialog",
  /** Close control on the detail dialog. */
  agentActivityDetailClose: "agent-activity-detail-close",
  /** The tool call's raw_input, prettified + color-highlighted JSON. */
  agentActivityDetailInput: "agent-activity-detail-input",
  /** The tool call's raw_output, prettified + color-highlighted JSON. */
  agentActivityDetailOutput: "agent-activity-detail-output",
  /** A color-highlighted JSON block (Prism output) inside the detail dialog. */
  agentActivityJsonHighlight: "agent-activity-json-highlight",
  /** Placeholder shown in place of a JSON body while `GetAcpToolCallDetail` is in flight. */
  agentActivityDetailSkeleton: "agent-activity-detail-skeleton",
  /** Note shown under the Output heading when the call has produced no output yet. */
  agentActivityDetailNoOutput: "agent-activity-detail-no-output",
  /** Note shown under the Input heading when the lookup resolved without an input body. */
  agentActivityDetailNoInput: "agent-activity-detail-no-input",
  /** Inline error shown when the detail lookup cannot be answered (entry carries no tool call id,
   *  unknown id, or transport failure). */
  agentActivityDetailError: "agent-activity-detail-error",

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

  // Daemon selector (top-right strip on daemon-mode screens)
  daemonSelectorTrigger: "daemon-selector-trigger",

  // Projects screen (/projects)
  projectsScreen: "projects-screen",
  projectsList: "projects-list",
  projectsCreateProjectToggle: "projects-create-project-toggle",
  projectsCreateProjectForm: "projects-create-project-form",
  projectsNewProjectName: "projects-new-project-name",
  projectsNewProjectGitUrl: "projects-new-project-git-url",
  projectsNewProjectUserRelativePath: "projects-new-project-user-relative-path",
  projectsCreateProjectSubmit: "projects-create-project-submit",
} as const;

// ---------------------------------------------------------------------------
// Dynamic test-id helpers (includes a session ID, project ID, etc.)
// ---------------------------------------------------------------------------

/** `[data-testid="sessions-table-<projectId>"]` */
export const sessionsTable = (projectId: string) => `sessions-table-${projectId}`;

/** `[data-testid="connect-<sessionId>"]` */
export const connectBtn = (sessionId: string) => `connect-${sessionId}`;

/** `[data-testid="create-session-subagent-checkbox-<name>"]` — one per row in the "Managed
 * codebase" specialized-subagent multi-select. See docs/ft/coder/specialized-subagents.md. */
export const createSessionSubagentCheckbox = (name: string) =>
  `create-session-subagent-checkbox-${name}`;

/** `[data-testid="delete-session-<sessionId>"]` */
export const deleteSessionBtn = (sessionId: string) => `delete-session-${sessionId}`;

/** `[data-testid="signal-dropdown-<sessionId>"]` */
export const signalDropdown = (sessionId: string) => `signal-dropdown-${sessionId}`;

/** `[data-testid="signal-menu-<sessionId>"]` */
export const signalMenu = (sessionId: string) => `signal-menu-${sessionId}`;

/** `[data-testid="signal-sigint-<sessionId>"]` */
export const signalSigint = (sessionId: string) => `signal-sigint-${sessionId}`;

/** `[data-testid="signal-sigterm-<sessionId>"]` */
export const signalSigterm = (sessionId: string) => `signal-sigterm-${sessionId}`;

/** `[data-testid="signal-sigkill-<sessionId>"]` */
export const signalSigkill = (sessionId: string) => `signal-sigkill-${sessionId}`;

/** `[data-testid="session-row-select-<sessionId>"]` */
export const sessionRowSelect = (sessionId: string) => `session-row-select-${sessionId}`;

/** `[data-testid="session-table-select-all-<projectId>"]` */
export const sessionTableSelectAll = (projectId: string) => `session-table-select-all-${projectId}`;

/** `[data-testid="bulk-delete-button-<projectId>"]` */
export const bulkDeleteButton = (projectId: string) => `bulk-delete-button-${projectId}`;

/** `[data-testid="backend-select-<projectId>"]` */
export const backendSelect = (projectId: string) => `backend-select-${projectId}`;

/** `[data-testid="host-select-<rowKey>"]` */
export const hostSelect = (rowKey: string) => `host-select-${rowKey}`;

/** `[data-testid="start-session-<projectId>"]` */
export const startSession = (projectId: string) => `start-session-${projectId}`;

/** `[data-testid="connection-attached-terminal-<sessionId>"]` */
export const attachedTerminal = (sessionId: string) => `connection-attached-terminal-${sessionId}`;

/** `[data-testid="participant-entry-<identity>"]` */
export const participantEntry = (identity: string) => `participant-entry-${identity}`;

/** `[data-testid="participant-role-<identity>"]` */
export const participantRole = (identity: string) => `participant-role-${identity}`;

/** `[data-testid="participant-metadata-<identity>"]` */
export const participantMetadata = (identity: string) => `participant-metadata-${identity}`;

/** `[data-testid="participant-video-trigger-<identity>"]` */
export const participantVideoTrigger = (identity: string) =>
  `participant-video-trigger-${identity}`;

/** `[data-testid="participant-codex-oauth-<identity>"]` */
export const participantCodexOauth = (identity: string) => `participant-codex-oauth-${identity}`;

/** `[data-testid="participant-owned-project-count-<identity>"]` */
export const participantOwnedProjectCount = (identity: string) =>
  `participant-owned-project-count-${identity}`;

/** `[data-testid="session-more-actions-<sessionId>"]` */
export const sessionMoreActions = (sessionId: string) => `session-more-actions-${sessionId}`;

// ---------------------------------------------------------------------------
// LiveKit rooms panel dynamic helpers
// ---------------------------------------------------------------------------

/**
 * Room names and participant identities carry characters that do not belong in a test id
 * (`livekit.common_room`, `daemon-x/y`), so the helpers below collapse them with `safeTestIdPart`.
 * `LiveKitRoomsPanel.tsx` and `ParticipantList.tsx` import that same function from
 * `src/lib/testId.ts`, so these selectors cannot drift from the ids they select.
 */

/** `[data-testid="livekit-room-entry-<room>"]` */
export const livekitRoomEntry = (room: string) => `livekit-room-entry-${safeTestIdPart(room)}`;

/** `[data-testid="livekit-room-toggle-<room>"]` */
export const livekitRoomToggle = (room: string) => `livekit-room-toggle-${safeTestIdPart(room)}`;

/** `[data-testid="livekit-room-name-<room>"]` */
export const livekitRoomName = (room: string) => `livekit-room-name-${safeTestIdPart(room)}`;

/** `[data-testid="livekit-room-label-<room>"]` */
export const livekitRoomLabel = (room: string) => `livekit-room-label-${safeTestIdPart(room)}`;

/** `[data-testid="livekit-room-participant-count-<room>"]` */
export const livekitRoomParticipantCount = (room: string) =>
  `livekit-room-participant-count-${safeTestIdPart(room)}`;

/** `[data-testid="livekit-room-created-at-<room>"]` */
export const livekitRoomCreatedAt = (room: string) =>
  `livekit-room-created-at-${safeTestIdPart(room)}`;

/** `[data-testid="livekit-room-no-participants-<room>"]` */
export const livekitRoomNoParticipants = (room: string) =>
  `livekit-room-no-participants-${safeTestIdPart(room)}`;

/** `[data-testid="livekit-room-participant-entry-<room>-<identity>"]` */
export const livekitRoomParticipantEntry = (room: string, identity: string) =>
  `livekit-room-participant-entry-${safeTestIdPart(room)}-${safeTestIdPart(identity)}`;

/** `[data-testid="livekit-room-participant-role-<room>-<identity>"]` */
export const livekitRoomParticipantRole = (room: string, identity: string) =>
  `livekit-room-participant-role-${safeTestIdPart(room)}-${safeTestIdPart(identity)}`;

/** `[data-testid="livekit-room-participant-joined-<room>-<identity>"]` */
export const livekitRoomParticipantJoined = (room: string, identity: string) =>
  `livekit-room-participant-joined-${safeTestIdPart(room)}-${safeTestIdPart(identity)}`;

/** `[data-testid="livekit-room-participant-state-<room>-<identity>"]` */
export const livekitRoomParticipantState = (room: string, identity: string) =>
  `livekit-room-participant-state-${safeTestIdPart(room)}-${safeTestIdPart(identity)}`;

/** `[data-testid="livekit-room-participant-metadata-<room>-<identity>"]` */
export const livekitRoomParticipantMetadata = (room: string, identity: string) =>
  `livekit-room-participant-metadata-${safeTestIdPart(room)}-${safeTestIdPart(identity)}`;

// ---------------------------------------------------------------------------
// Sessions drawer screen dynamic helpers
// ---------------------------------------------------------------------------

/** `[data-testid="sessions-drawer-item-<sessionId>"]` — clickable drawer row */
export const sessionsDrawerItem = (sessionId: string) => `sessions-drawer-item-${sessionId}`;

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

/** `[data-testid="worktree-tree-node-<relPath>"]` — a single file/dir node in the worktree tree. */
export const worktreeTreeNode = (relPath: string) => `worktree-tree-node-${relPath}`;

/** `[data-testid="sessions-terminal-tab-<terminalId>"]` — a single bash terminal tab. */
export const sessionsTerminalTab = (terminalId: string) =>
  `sessions-terminal-tab-${terminalId}`;

/** `[data-testid="sessions-terminal-tab-close-<terminalId>"]` — the ✕ close control on a bash tab. */
export const sessionsTerminalTabClose = (terminalId: string) =>
  `sessions-terminal-tab-close-${terminalId}`;

/** `[data-testid="sessions-terminal-pane-<terminalId>"]` — the mounted terminal container for one
 *  terminal_id (active is CSS-visible, the rest are `display:none` but stay mounted). The Agent tab
 *  uses `terminalId = "main"`. */
export const sessionsTerminalPane = (terminalId: string) =>
  `sessions-terminal-pane-${terminalId}`;

/** `[data-testid="sessions-child-tab-<sessionId>"]` — a tab for a spawned child conversation
 *  (a child session whose `orchestratorSessionId` is the parent runtime's session). */
export const sessionsChildTab = (sessionId: string) => `sessions-child-tab-${sessionId}`;

/** `[data-testid="sessions-child-pane-<sessionId>"]` — the mounted runtime pane for a spawned
 *  child conversation, shown when its tab is selected. */
export const sessionsChildPane = (sessionId: string) => `sessions-child-pane-${sessionId}`;

/** `[data-testid="session-agents-row-<sessionId>"]` — one row in the Session agents section, keyed
 *  by the peer's session id. */
export const sessionAgentsRow = (sessionId: string) => `session-agents-row-${sessionId}`;

/** `[data-testid="session-agents-switch-btn-<sessionId>"]` — the switch action on a peer row. */
export const sessionAgentsSwitchBtn = (sessionId: string) =>
  `session-agents-switch-btn-${sessionId}`;

// ---------------------------------------------------------------------------
// Tasks drawer screen dynamic helpers
// ---------------------------------------------------------------------------

/** `[data-testid="tasks-drawer-item-<taskId>"]` — clickable drawer row */
export const tasksDrawerItem = (taskId: string) => `tasks-drawer-item-${taskId}`;

/** `[data-testid="tasks-drawer-item-status-<taskId>"]` — status indicator dot */
export const tasksDrawerItemStatus = (taskId: string) => `tasks-drawer-item-status-${taskId}`;

/** `[data-testid="tasks-drawer-item-kind-<taskId>"]` — kind text */
export const tasksDrawerItemKind = (taskId: string) => `tasks-drawer-item-kind-${taskId}`;

/** `[data-testid="tasks-drawer-item-cancel-<taskId>"]` — inline Cancel button in drawer row */
export const tasksDrawerItemCancel = (taskId: string) => `tasks-drawer-item-cancel-${taskId}`;

/** `[data-testid="tasks-output-pane-status-<taskId>"]` — status label in output pane */
export const tasksOutputPaneStatus = (taskId: string) => `tasks-output-pane-status-${taskId}`;

/** `[data-testid="tasks-output-pane-cancel-<taskId>"]` — Cancel button in output pane */
export const tasksOutputPaneCancel = (taskId: string) => `tasks-output-pane-cancel-${taskId}`;

/** `[data-testid="tasks-channel-tab-<taskId>-<channelId>"]` — channel tab */
export const tasksChannelTab = (taskId: string, channelId: string) =>
  `tasks-channel-tab-${taskId}-${channelId}`;

/** `[data-testid="tasks-channel-output-<taskId>-<channelId>"]` — channel output area */
export const tasksChannelOutput = (taskId: string, channelId: string) =>
  `tasks-channel-output-${taskId}-${channelId}`;

/** `[data-testid="shortcut-button-<label>"]` — individual shortcut button */
export const shortcutButton = (label: string) => `shortcut-button-${label}`;

// ---------------------------------------------------------------------------
// VNC tab dynamic helpers
// ---------------------------------------------------------------------------

/** `[data-testid="sessions-vnc-target-row-<targetId>"]` — a single VNC target row */
export const sessionsVncTargetRow = (targetId: string) =>
  `sessions-vnc-target-row-${targetId}`;

/** `[data-testid="sessions-vnc-start-<targetId>"]` — Start stream button */
export const sessionsVncStartBtn = (targetId: string) => `sessions-vnc-start-${targetId}`;

/** `[data-testid="sessions-vnc-stop-<targetId>"]` — Stop stream button */
export const sessionsVncStopBtn = (targetId: string) => `sessions-vnc-stop-${targetId}`;

/** `[data-testid="sessions-vnc-remove-<targetId>"]` — Remove target button */
export const sessionsVncRemoveBtn = (targetId: string) => `sessions-vnc-remove-${targetId}`;

// ---------------------------------------------------------------------------
// Usage tab dynamic helpers (one row per conversation, keyed by conversation id)
// ---------------------------------------------------------------------------

/** `[data-testid="sessions-usage-row-<id>"]` — a single conversation row */
export const sessionsUsageRow = (id: string) => `sessions-usage-row-${id}`;

/** `[data-testid="sessions-usage-row-agent-<id>"]` — the agent cell of a conversation row */
export const sessionsUsageRowAgent = (id: string) => `sessions-usage-row-agent-${id}`;

/** `[data-testid="sessions-usage-row-model-<id>"]` — the model cell of a conversation row */
export const sessionsUsageRowModel = (id: string) => `sessions-usage-row-model-${id}`;

/** `[data-testid="sessions-usage-row-input-<id>"]` — the input-tokens cell of a conversation row */
export const sessionsUsageRowInput = (id: string) => `sessions-usage-row-input-${id}`;

/** `[data-testid="sessions-usage-row-output-<id>"]` — the output-tokens cell of a conversation row */
export const sessionsUsageRowOutput = (id: string) => `sessions-usage-row-output-${id}`;

/** `[data-testid="sessions-usage-row-total-<id>"]` — the total-tokens cell of a conversation row */
export const sessionsUsageRowTotal = (id: string) => `sessions-usage-row-total-${id}`;

/** `[data-testid="sessions-usage-row-turns-<id>"]` — the turns cell of a conversation row */
export const sessionsUsageRowTurns = (id: string) => `sessions-usage-row-turns-${id}`;

// ---------------------------------------------------------------------------
// Screen Sharing tab dynamic helpers
// ---------------------------------------------------------------------------

/** `[data-testid="sessions-screen-sharing-target-row-<targetId>"]` */
export const sessionsScreenSharingTargetRow = (targetId: string) =>
  `sessions-screen-sharing-target-row-${targetId}`;

/** `[data-testid="sessions-screen-sharing-start-<targetId>"]` — Start stream button */
export const sessionsScreenSharingStartBtn = (targetId: string) =>
  `sessions-screen-sharing-start-${targetId}`;

/** `[data-testid="sessions-screen-sharing-stop-<targetId>"]` — Stop stream button */
export const sessionsScreenSharingStopBtn = (targetId: string) =>
  `sessions-screen-sharing-stop-${targetId}`;

/** `[data-testid="sessions-screen-sharing-remove-<targetId>"]` — Remove target button */
export const sessionsScreenSharingRemoveBtn = (targetId: string) =>
  `sessions-screen-sharing-remove-${targetId}`;

// ---------------------------------------------------------------------------
// PR-Stack Chat Screen dynamic helpers
// ---------------------------------------------------------------------------

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
 *  be started. The row's information and its (disabled) Start-session CTA stay put beside it */
export const prStackStartWarning = (nodeId: string) => `pr-stack-start-warning-${nodeId}`;

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

/** `[data-testid="agent-activity-row-<callId>"]` — one one-line record row in the activity overlay */
export const agentActivityRow = (callId: string) => `agent-activity-row-${callId}`;

/** `[data-testid="pr-stack-add-planned-pr-ancestor-<nodeId>"]` — an ancestor checkbox in the "New planned PR" form */
export const prStackAddPlannedPrAncestorCheckbox = (nodeId: string) =>
  `pr-stack-add-planned-pr-ancestor-${nodeId}`;

// ---------------------------------------------------------------------------
// Projects screen dynamic helpers
// ---------------------------------------------------------------------------

/** `[data-testid="project-card-<projectId>"]` — one card per logical project (may span hosts) */
export const projectCard = (projectId: string) => `project-card-${projectId}`;

/** `[data-testid="project-host-row-<projectId>-<daemonInstanceId>"]` — one row per hosting daemon */
export const projectHostRow = (projectId: string, daemonInstanceId: string) =>
  `project-host-row-${projectId}-${daemonInstanceId}`;

/** `[data-testid="project-add-to-host-toggle-<projectId>"]` — opens the add-to-host control */
export const projectAddToHostToggle = (projectId: string) =>
  `project-add-to-host-toggle-${projectId}`;

/** `[data-testid="project-add-to-host-select-<projectId>"]` — target-host `<select>` */
export const projectAddToHostSelect = (projectId: string) =>
  `project-add-to-host-select-${projectId}`;

/** `[data-testid="project-add-to-host-submit-<projectId>"]` — submits the add-to-host action */
export const projectAddToHostSubmit = (projectId: string) =>
  `project-add-to-host-submit-${projectId}`;

/** `[data-testid="project-add-to-host-user-relative-path-<projectId>"]` — optional clone-location
 *  input in the add-to-host control (path relative to the target host's home). */
export const projectAddToHostUserRelativePath = (projectId: string) =>
  `project-add-to-host-user-relative-path-${projectId}`;

/** `[data-testid="project-host-base-location-<daemonInstanceId>"]` — a daemon's advertised base
 *  clone location (repos_base_path), surfaced in the Projects screen. */
export const projectHostBaseLocation = (daemonInstanceId: string) =>
  `project-host-base-location-${daemonInstanceId}`;

/** `[data-testid="project-default-branch-select-<projectId>"]` — the project's default-branch
 *  (`main_branch_ref`) dropdown, listing the project's remote branches. */
export const projectDefaultBranchSelect = (projectId: string) =>
  `project-default-branch-select-${projectId}`;

// ---------------------------------------------------------------------------
// Daemon selector dynamic helpers
// ---------------------------------------------------------------------------

/** `[data-testid="daemon-selector-option-<instanceId>"]` — one option in the daemon selector */
export const daemonSelectorOption = (instanceId: string) => `daemon-selector-option-${instanceId}`;

// ---------------------------------------------------------------------------
// Host stats footer dynamic helpers
// ---------------------------------------------------------------------------

/** `[data-testid="cpu-core-bar-<index>"]` — the mini bar for logical core `<index>` (0-based). */
export const cpuCoreBar = (index: number) => `cpu-core-bar-${index}`;

// ---------------------------------------------------------------------------
// Session Inspector → Files tab dynamic helpers (docs/ft/web/session-files-inspector.md)
// ---------------------------------------------------------------------------

/** `[data-testid="session-upload-row-<fileName>"]` — one uploaded-file row. */
export const sessionUploadRow = (fileName: string) => `session-upload-row-${fileName}`;

/** The size readout inside an uploaded-file row. */
export const sessionUploadSize = (fileName: string) => `session-upload-size-${fileName}`;

/** The Insert-into-terminal button of an uploaded-file row. */
export const sessionUploadInsert = (fileName: string) => `session-upload-insert-${fileName}`;

/** The Copy-host-path button of an uploaded-file row. */
export const sessionUploadCopyPath = (fileName: string) => `session-upload-copy-path-${fileName}`;

/** The Delete button (first step) of an uploaded-file row. */
export const sessionUploadDelete = (fileName: string) => `session-upload-delete-${fileName}`;

/** The Confirm-delete button (second step) of an uploaded-file row. */
export const sessionUploadDeleteConfirm = (fileName: string) =>
  `session-upload-delete-confirm-${fileName}`;

// ---------------------------------------------------------------------------
// New-session attachment rows — one per document attached at creation time.
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Models & Agents dynamic helpers
// ---------------------------------------------------------------------------

/**
 * `[data-testid="models-provider-row-<daemonInstanceId>-<providerId>"]` — one row per configured
 * provider. Provider ids are minted per daemon (`prov-ollama` exists on every host), so the owning
 * daemon is part of the id; without it two hosts' rows would collide in the DOM.
 */
export const modelsProviderRow = (daemonInstanceId: string, providerId: string) =>
  `models-provider-row-${daemonInstanceId}-${providerId}`;

/** The inline enumeration error rendered against a provider whose last refresh failed. */
export const modelsProviderError = (daemonInstanceId: string, providerId: string) =>
  `${modelsProviderRow(daemonInstanceId, providerId)}-error`;

/** Whether a credential is stored for a provider — never the credential itself. */
export const modelsProviderCredential = (daemonInstanceId: string, providerId: string) =>
  `${modelsProviderRow(daemonInstanceId, providerId)}-credential`;

/** Re-enumerates one provider's models from the provider itself. */
export const modelsProviderRefresh = (daemonInstanceId: string, providerId: string) =>
  `${modelsProviderRow(daemonInstanceId, providerId)}-refresh`;

/**
 * `[data-testid="models-row-<daemonInstanceId>-<providerId>-<modelId>"]` — one row per model.
 * A model id may contain a colon (`qwen3:32b`), so it goes through `safeTestIdPart`.
 */
export const modelsRow = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `models-row-${daemonInstanceId}-${providerId}-${safeTestIdPart(modelId)}`;

/** The owning-daemon cell of a model row. */
export const modelsRowDaemon = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-daemon`;

/** The capability-label cell of a model row; carries `data-model-labels` for exact assertions. */
export const modelsRowLabels = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-labels`;

/** The load-state cell of a model row; carries `data-load-state`. */
export const modelsRowLoadState = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-load-state`;

/** The Load action on a model row (rendered only when the model is not loaded). */
export const modelsRowLoad = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-load`;

/** The Unload action on a model row (rendered only when the model is loaded). */
export const modelsRowUnload = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-unload`;

/** The Chat action on a model row (rendered only for chat-capable models). */
export const modelsRowChat = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-chat`;

/** The "create assistant from this model" action on a model row. */
export const modelsRowCreateAssistant = (
  daemonInstanceId: string,
  providerId: string,
  modelId: string,
) => `${modelsRow(daemonInstanceId, providerId, modelId)}-create-assistant`;

/** The per-row error surfaced when an action is rejected by the daemon. */
export const modelsRowError = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-error`;

/** The marker rendered on a model row whose provider's last enumeration failed. */
export const modelsRowStale = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-stale`;

/** The error row rendered for a daemon whose registry could not be read. */
export const modelsDaemonError = (daemonInstanceId: string) =>
  `models-daemon-error-${daemonInstanceId}`;

/** One assistant row, keyed by the assistant's `--agent` name. */
export const modelsAssistantRow = (name: string) => `models-assistant-row-${name}`;

/** The assigned-tools cell of an assistant row; carries `data-assistant-tools`. */
export const modelsAssistantTools = (name: string) => `models-assistant-tools-${name}`;

/** One selectable tool checkbox in the create-assistant dialog. */
export const modelsCreateAssistantTool = (toolName: string) =>
  `models-create-assistant-tool-${toolName}`;
