/**
 * Centralised data-testid constants.
 *
 * Every `data-testid` value used by the Cypress suite lives here — the raw string
 * appears once; tests use the named constant so a rename is a one-line change.
 *
 * For identifiers that include a dynamic segment (session ID, project ID, …) the
 * constant is a prefix or a helper function — see the examples below.
 */

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

  // ConnectionScreen / session table
  sessionsTableOrphan: "sessions-table-orphan",

  // Participants
  participantList: "participant-list",
  participantListEmpty: "participant-list-empty",
  participantListError: "participant-list-error",
  connectedParticipantsPanel: "connected-participants-panel",

  // Worktrees
  shellMenuWorktrees: "shell-menu-worktrees",
  worktreesScreen: "worktrees-screen",
  worktreesTable: "worktrees-table",
  worktreeRow: "worktrees-row",
  worktreeDelete: "worktrees-delete",
  worktreeDeleteConfirm: "worktrees-delete-confirm",
  worktreeDeletedPath: "worktrees-deleted-path",

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

  // Sessions drawer screen
  sessionsDrawerScreen: "sessions-drawer-screen",
  sessionsDrawer: "sessions-drawer",
  sessionsDetailPane: "sessions-detail-pane",
  sessionsDetailTerminalContainer: "sessions-detail-terminal-container",
  sessionsDetailMetadata: "sessions-detail-metadata",

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

  // Sessions drawer — open/close toggle
  sessionsDrawerCloseBtn: "sessions-drawer-close-btn",
  sessionsDrawerOpenBtn: "sessions-drawer-open-btn",
  sessionsDrawerOpenOverlayBtn: "sessions-drawer-open-overlay-btn",

  // Sessions drawer — create session
  sessionsDrawerNewBtn: "sessions-drawer-new-btn",
  createSessionPane: "create-session-pane",
  createSessionTypeToolBtn: "create-session-type-tool",
  createSessionTypeClaudeCliBtn: "create-session-type-claude-cli",
  createSessionProjectSelect: "create-session-project-select",
  createSessionAgentSelect: "create-session-agent-select",
  createSessionRecipeInput: "create-session-recipe-input",
  /** Replaces the free-text recipe input for tool sessions — a <select> with all 7 recipe options. */
  createSessionRecipeSelect: "create-session-recipe-select",
  /** Parent-picker <select> — lists sessions that act as orchestrators; tool sessions only. */
  createSessionStackParentSelect: "create-session-stack-parent-select",
  createSessionModelSelect: "create-session-model-select",
  createSessionPermissionModeSelect: "create-session-permission-mode-select",
  createSessionSandboxToggle: "create-session-sandbox-toggle",
  /** Collapsible "Managed codebase" section header — claude-cli sessions only. See
   * docs/ft/coder/specialized-subagents.md. */
  createSessionManagedCodebaseToggle: "create-session-managed-codebase-toggle",
  /** Expanded "Managed codebase" section content (specialized-subagent multi-select). */
  createSessionManagedCodebaseSection: "create-session-managed-codebase-section",
  createSessionInitialPromptInput: "create-session-initial-prompt-input",
  createSessionBranchIntentSelect: "create-session-branch-intent-select",
  createSessionNewBranchNameInput: "create-session-new-branch-name-input",
  createSessionBranchToWorkOnSelect: "create-session-branch-to-work-on-select",
  createSessionCancelBtn: "create-session-cancel-btn",
  createSessionSubmitBtn: "create-session-submit-btn",
  createSessionError: "create-session-error",

  // Shell navigation
  shellMenuButton: "shell-menu-button",
  shellMenuRpcPlayground: "shell-menu-rpc-playground",

  // RPC Playground
  rpcPlaygroundParticipantSelect: "rpc-playground-participant-select",
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

  // Session traffic strip
  sessionTrafficStrip: "session-traffic-strip",
  sessionTrafficBytesIn: "session-traffic-bytes-in",
  sessionTrafficBytesOut: "session-traffic-bytes-out",
  sessionTrafficRateIn: "session-traffic-rate-in",
  sessionTrafficRateOut: "session-traffic-rate-out",
  sessionTrafficPing: "session-traffic-ping",

  // Terminal control mutex — "Claim terminal" CTA
  terminalControlOverlay: "terminal-control-overlay",
  terminalClaimBtn: "terminal-claim-btn",
  terminalControlHolder: "terminal-control-holder",

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
  prStackChat: "pr-stack-chat",
  prStackChatMessages: "pr-stack-chat-messages",
  prStackChatInput: "pr-stack-chat-input",
  prStackChatSendBtn: "pr-stack-chat-send-btn",
  prStackChatError: "pr-stack-chat-error",
  prStackChatConnecting: "pr-stack-chat-connecting",
  prStackChatStatus: "pr-stack-chat-status",
  prStackChatActivity: "pr-stack-chat-activity",

  // PR-Stack Chat Screen — clarification question elicitation (AppMode::Select / MultiSelect)
  prStackChatQuestion: "pr-stack-chat-question",
  prStackChatQuestionHeader: "pr-stack-chat-question-header",
  prStackChatQuestionText: "pr-stack-chat-question-text",
  prStackChatQuestionOtherInput: "pr-stack-chat-question-other-input",
  prStackChatQuestionOtherSubmit: "pr-stack-chat-question-other-submit",
  prStackChatMultiSelectSubmit: "pr-stack-chat-multiselect-submit",

  // PR-Stack Chat Screen — manually adding a planned PR (deterministic, non-chat path)
  prStackAddPlannedPrBtn: "pr-stack-add-planned-pr-btn",
  prStackAddPlannedPrForm: "pr-stack-add-planned-pr-form",
  prStackAddPlannedPrTitleInput: "pr-stack-add-planned-pr-title-input",
  prStackAddPlannedPrDescriptionInput: "pr-stack-add-planned-pr-description-input",
  prStackAddPlannedPrBranchSuggestionInput: "pr-stack-add-planned-pr-branch-suggestion-input",
  prStackAddPlannedPrSubmitBtn: "pr-stack-add-planned-pr-submit-btn",
  prStackAddPlannedPrCancelBtn: "pr-stack-add-planned-pr-cancel-btn",
  prStackAddPlannedPrError: "pr-stack-add-planned-pr-error",

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

/** `[data-testid="sessions-detail-resume-<sessionId>"]` — Resume button in detail pane */
export const sessionsDetailResumeBtn = (sessionId: string) =>
  `sessions-detail-resume-${sessionId}`;

/** `[data-testid="sessions-detail-delete-<sessionId>"]` — Delete button in detail pane */
export const sessionsDetailDeleteBtn = (sessionId: string) =>
  `sessions-detail-delete-${sessionId}`;

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

/** `[data-testid="pr-stack-chat-message-<index>"]` — a single rendered chat bubble */
export const prStackChatMessage = (index: number) => `pr-stack-chat-message-${index}`;

/** `[data-testid="pr-stack-chat-option-<index>"]` — a single-select option button */
export const prStackChatOption = (index: number) => `pr-stack-chat-option-${index}`;

/** `[data-testid="pr-stack-chat-multiselect-option-<index>"]` — a multi-select option checkbox */
export const prStackChatMultiSelectOption = (index: number) => `pr-stack-chat-multiselect-option-${index}`;

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

// ---------------------------------------------------------------------------
// Daemon selector dynamic helpers
// ---------------------------------------------------------------------------

/** `[data-testid="daemon-selector-option-<instanceId>"]` — one option in the daemon selector */
export const daemonSelectorOption = (instanceId: string) => `daemon-selector-option-${instanceId}`;
