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
  ghosttyTerminal: "ghostty-terminal",
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

