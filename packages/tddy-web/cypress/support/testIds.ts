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

  // Tasks drawer screen
  tasksDrawerScreen: "tasks-drawer-screen",
  tasksDrawer: "tasks-drawer",
  tasksOutputPane: "tasks-output-pane",
  tasksOutputPaneEmpty: "tasks-output-pane-empty",

  // Sessions drawer — create session
  sessionsDrawerNewBtn: "sessions-drawer-new-btn",
  createSessionPane: "create-session-pane",
  createSessionTypeToolBtn: "create-session-type-tool",
  createSessionTypeClaudeCliBtn: "create-session-type-claude-cli",
  createSessionProjectSelect: "create-session-project-select",
  createSessionAgentSelect: "create-session-agent-select",
  createSessionRecipeInput: "create-session-recipe-input",
  createSessionModelSelect: "create-session-model-select",
  createSessionPermissionModeSelect: "create-session-permission-mode-select",
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
} as const;

// ---------------------------------------------------------------------------
// Dynamic test-id helpers (includes a session ID, project ID, etc.)
// ---------------------------------------------------------------------------

/** `[data-testid="sessions-table-<projectId>"]` */
export const sessionsTable = (projectId: string) => `sessions-table-${projectId}`;

/** `[data-testid="connect-<sessionId>"]` */
export const connectBtn = (sessionId: string) => `connect-${sessionId}`;

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
