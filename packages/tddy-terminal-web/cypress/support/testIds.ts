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
  livekitStatus: "livekit-status",
  buildId: "build-id",

  // Terminal chrome
  connectionStatusDot: "connection-status-dot",
  connectionMenuDisconnect: "connection-menu-disconnect",
  connectionMenuTerminate: "connection-menu-terminate",
  ghosttyTerminal: "ghostty-terminal",
  terminalFullscreenButton: "terminal-fullscreen-button",
  terminalConnectionStatusBar: "terminal-connection-status-bar",
  mobileKeyboardButton: "mobile-keyboard-button",
  terminalDropOverlay: "terminal-drop-overlay",
  terminalDropRefused: "terminal-drop-refused",
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
  /** Hidden, readable mirror of what the LIVE terminal has painted (the canvas is WebGL). */
  terminalBufferText: "terminal-buffer-text",
  /** Cover shown over the pane once the far end has ended the session. */
  terminalCoderUnavailable: "terminal-coder-unavailable",
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
  shortcutDrawer: "shortcut-drawer",
  shortcutDragHandle: "shortcut-drag-handle",

  // Host stats footer (screen-level bottom strip: relocated traffic + disk + per-core CPU)
  hostStatsFooter: "host-stats-footer",
  /** Free-space readout for the daemon's default project directory filesystem. */
} as const;

export const sessionsAgentTab = (conversationId: string) =>
  `sessions-agent-tab-${safeTestIdPart(conversationId)}`;

/** `[data-testid="sessions-agent-tab-<conversationId>-close"]` — the ✕ on an agent tab. Closing it
 *  cancels the conversation. Suffixed rather than infixed so the tab-listing selectors can exclude
 *  the close controls with a `$=` match. */
export const sessionAgentPickerOption = (agentId: string) =>
  `session-agent-picker-option-${safeTestIdPart(agentId)}`;

/** The owning-daemon label on a header-picker option. */
export const shortcutButton = (label: string) => `shortcut-button-${label}`;

// ---------------------------------------------------------------------------
// VNC tab dynamic helpers
// ---------------------------------------------------------------------------

/** `[data-testid="sessions-vnc-target-row-<targetId>"]` — a single VNC target row */
export const modelsProviderRow = (daemonInstanceId: string, providerId: string) =>
  `models-provider-row-${daemonInstanceId}-${providerId}`;

/** The inline enumeration error rendered against a provider whose last refresh failed. */
export const modelsRow = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `models-row-${daemonInstanceId}-${providerId}-${safeTestIdPart(modelId)}`;

/** The owning-daemon cell of a model row. */
export const modelsAssistantRow = (daemonInstanceId: string, name: string) =>
  `models-assistant-row-${daemonInstanceId}-${safeTestIdPart(name)}`;

/** The assigned-tools cell of an assistant row; carries `data-assistant-tools`. */
export const modelsChatMessage = (index: number) => `models-chat-message-${index}`;

/** A tool bubble's status marker; carries `data-tool-status`. Absent on non-tool bubbles. */
export const agentRosterRow = (agentId: string) => `agent-roster-row-${safeTestIdPart(agentId)}`;

/** The owning-daemon label on an attached agent's row. */
export const agentTreeSession = (sessionId: string) =>
  `agent-tree-session-${safeTestIdPart(sessionId)}`;

/**
 * What a subagent session's agent is doing, inferred by the daemon from that session's own
 * conversation (`SessionEntry.agent_status`). Carries the same `data-agent-status` tokens a roster
 * row does, because it is the same enum.
 */
export const agentRosterPickerOption = (agentId: string) =>
  `agent-roster-picker-option-${safeTestIdPart(agentId)}`;

/** The owning-daemon label on a picker option. */
export const createSessionAgentOption = (agentId: string) =>
  `create-session-agent-${safeTestIdPart(agentId)}`;

/** The owning-daemon label on a create-session agent option. */
