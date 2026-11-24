/**
 * Centralised data-testid constants for this package's Cypress suite — the PR-stack and agent chat
 * ids only; the app shell's ids stay with `tddy-web`.
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
// Test ids this package's specs read
// ---------------------------------------------------------------------------

export const TEST_IDS = {
  createSessionPane: "create-session-pane",
  createSessionDialog: "create-session-dialog",
  createSessionInitialPromptInput: "create-session-initial-prompt-input",
  createSessionTypeClaudeCliBtn: "create-session-type-claude-cli",
  createSessionBranchIntentSelect: "create-session-branch-intent-select",
  createSessionNewBranchNameInput: "create-session-new-branch-name-input",
  createSessionBranchToWorkOnSelect: "create-session-branch-to-work-on-select",
  createSessionCreateRemoteBranchToggle: "create-session-create-remote-branch-toggle",
  createSessionBaseBranchSelect: "create-session-base-branch-select",
  createSessionSubmitBtn: "create-session-submit-btn",
  /** The stack view — one project stack opened by its id (`#orchestrators` 9/11). */
  workQueueView: "work-queue-view",
  /** Why the queue could not be loaded — shown instead of an empty queue. */
  workQueueLoadError: "work-queue-load-error",
  /** The loaded queue is no longer live: reconnecting, or ended with a reason. */
  workQueueLiveNotice: "work-queue-live-notice",
  prStackPlannedPrList: "work-item-list",
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
  agentChatJumpToLatest: "agent-chat-jump-to-latest",
  agentChatOlderLoading: "agent-chat-older-loading",
  agentChatScrollState: "agent-chat-scroll-state",
  prStackAddPlannedPrBtn: "work-item-add-btn",
  prStackAddPlannedPrForm: "work-item-add-form",
  prStackAddPlannedPrTitleInput: "work-item-add-title-input",
  prStackAddPlannedPrDescriptionInput: "work-item-add-description-input",
  prStackAddPlannedPrBranchSuggestionInput: "work-item-add-branch-suggestion-input",
  prStackAddPlannedPrSubmitBtn: "work-item-add-submit-btn",
  prStackAddPlannedPrStartBtn: "work-item-add-start-btn",
  prStackAddPlannedPrCancelBtn: "work-item-add-cancel-btn",
  prStackAddPlannedPrError: "work-item-add-error",
  prStackDirtyWorktreeDialog: "work-item-dirty-worktree-dialog",
  prStackDirtyWorktreePaths: "work-item-dirty-worktree-paths",
  prStackDirtyWorktreeCommitMessageInput: "work-item-dirty-worktree-commit-message-input",
  prStackDirtyWorktreeCommitBtn: "work-item-dirty-worktree-commit-btn",
  prStackDirtyWorktreeCancelBtn: "work-item-dirty-worktree-cancel-btn",
} as const;

export const prStackPlannedPrRow = (itemId: string) => `work-item-row-${itemId}`;

export const prStackStartSessionBtn = (itemId: string) => `work-item-start-session-${itemId}`;

export const prStackStatusChip = (itemId: string) => `work-item-status-chip-${itemId}`;

export const prStackInternalStatusBadge = (itemId: string) =>
  `work-item-internal-status-badge-${itemId}`;

export const prStackInProgressBadge = (itemId: string) => `work-item-in-progress-${itemId}`;

export const prStackPrLink = (itemId: string) => `work-item-pr-link-${itemId}`;

export const prStackPrState = (itemId: string) => `work-item-pr-state-${itemId}`;

export const prStackRepointBtn = (itemId: string) => `work-item-repoint-${itemId}`;

export const prStackWorktree = (itemId: string) => `work-item-worktree-${itemId}`;

export const prStackSession = (itemId: string) => `work-item-session-${itemId}`;

export const prStackRowToggle = (itemId: string) => `work-item-toggle-${itemId}`;

export const prStackRowDetails = (itemId: string) => `work-item-details-${itemId}`;

export const prStackNodeId = (itemId: string) => `work-item-node-id-${itemId}`;

export const prStackParents = (itemId: string) => `work-item-parents-${itemId}`;

export const prStackChildRecipe = (itemId: string) => `work-item-child-recipe-${itemId}`;

export const prStackChildSession = (itemId: string) => `work-item-child-session-${itemId}`;

export const prStackChildState = (itemId: string) => `work-item-child-state-${itemId}`;

export const prStackMoveUpBtn = (itemId: string) => `work-item-move-up-${itemId}`;

export const prStackMoveDownBtn = (itemId: string) => `work-item-move-down-${itemId}`;

export const prStackReorderError = (itemId: string) => `work-item-reorder-error-${itemId}`;

export const prStackBaseBehind = (itemId: string) => `work-item-base-behind-${itemId}`;

export const prStackBaseInSync = (itemId: string) => `work-item-base-in-sync-${itemId}`;

export const prStackBaseConflicts = (itemId: string) => `work-item-base-conflicts-${itemId}`;

export const prStackBaseSyncUnavailable = (itemId: string) => `work-item-base-sync-unavailable-${itemId}`;

export const prStackBaseConflictPaths = (itemId: string) => `work-item-base-conflict-paths-${itemId}`;

export const prStackSyncMergeBtn = (itemId: string) => `work-item-sync-merge-${itemId}`;

export const prStackSyncRebaseBtn = (itemId: string) => `work-item-sync-rebase-${itemId}`;

export const prStackSyncError = (itemId: string) => `work-item-sync-error-${itemId}`;

export const prStackBranch = (itemId: string) => `work-item-branch-${itemId}`;

export const prStackPlannedBranch = (itemId: string) => `work-item-planned-branch-${itemId}`;

export const prStackBaseBranch = (itemId: string) => `work-item-base-branch-${itemId}`;

export const prStackStartWarning = (itemId: string) => `work-item-start-warning-${itemId}`;

export const prStackStartSessionBlockedIcon = (itemId: string) =>
  `work-item-start-session-blocked-icon-${itemId}`;

export const prStackRepointError = (itemId: string) => `work-item-repoint-error-${itemId}`;

export const prStackPrUnavailable = (itemId: string) => `work-item-pr-unavailable-${itemId}`;

export const agentChatMessage = (index: number) => `agent-chat-message-${index}`;

export const agentChatOption = (index: number) => `agent-chat-option-${index}`;

export const agentChatMultiSelectOption = (index: number) => `agent-chat-multiselect-option-${index}`;

export const agentChatElapsed = (index: number) => `agent-chat-elapsed-${index}`;

export const agentChatToolStatus = (index: number) => `agent-chat-tool-status-${index}`;

export const prStackAddPlannedPrAncestorCheckbox = (itemId: string) =>
  `work-item-add-ancestor-${itemId}`;
