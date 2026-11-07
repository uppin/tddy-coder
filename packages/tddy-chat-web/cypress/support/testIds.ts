/**
 * Centralised data-testid constants for this package's Cypress suite — the agent chat and activity
 * ids only; the app shell's ids stay with `tddy-web`.
 */

/** Build a `cy.get` selector for `[data-testid='<id>']`. */
export const byTestId = (
  id: string,
  options?: Parameters<typeof cy.get>[1],
): Cypress.Chainable<JQuery<HTMLElement>> =>
  cy.get(`[data-testid='${id}']`, options);

export const TEST_IDS = {
  // Reusable Agent Chat (recipe-agnostic; the PR-Stack chat view renders it via PrStackChat)
  agentChat: "agent-chat",
  agentChatMessages: "agent-chat-messages",
  agentChatInput: "agent-chat-input",
  agentChatSendBtn: "agent-chat-send-btn",
  agentChatExportBtn: "agent-chat-export-btn",
  agentChatError: "agent-chat-error",
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
  /** The Agent Activity transcript docked as the session view's right sidebar (`#project-home` 3/7). */
  agentActivitySidebar: "agent-activity-sidebar",
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
} as const;

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
