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
  createSessionBranchIntentSelect: "create-session-branch-intent-select",
  createSessionNewBranchNameInput: "create-session-new-branch-name-input",
  createSessionBranchToWorkOnSelect: "create-session-branch-to-work-on-select",
  createSessionCreateRemoteBranchToggle: "create-session-create-remote-branch-toggle",
  createSessionBaseBranchSelect: "create-session-base-branch-select",
  createSessionSubmitBtn: "create-session-submit-btn",
  prStackScreen: "pr-stack-screen",
  /** The stack view — one project stack opened by its id (`#orchestrators` 9/11). */
  projectStackView: "project-stack-view",
  prStackPlannedPrList: "pr-stack-planned-pr-list",
  prStackPlannedPrPanel: "pr-stack-planned-pr-panel",
  prStackPlannedPrPanelToggle: "pr-stack-planned-pr-panel-toggle",
  prStackPlannedPrPanelClose: "pr-stack-planned-pr-panel-close",
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
  prStackAddPlannedPrBtn: "pr-stack-add-planned-pr-btn",
  prStackAddPlannedPrForm: "pr-stack-add-planned-pr-form",
  prStackAddPlannedPrTitleInput: "pr-stack-add-planned-pr-title-input",
  prStackAddPlannedPrDescriptionInput: "pr-stack-add-planned-pr-description-input",
  prStackAddPlannedPrBranchSuggestionInput: "pr-stack-add-planned-pr-branch-suggestion-input",
  prStackAddPlannedPrSubmitBtn: "pr-stack-add-planned-pr-submit-btn",
  prStackAddPlannedPrStartBtn: "pr-stack-add-planned-pr-start-btn",
  prStackAddPlannedPrCancelBtn: "pr-stack-add-planned-pr-cancel-btn",
  prStackAddPlannedPrError: "pr-stack-add-planned-pr-error",
  prStackDirtyWorktreeDialog: "pr-stack-dirty-worktree-dialog",
  prStackDirtyWorktreePaths: "pr-stack-dirty-worktree-paths",
  prStackDirtyWorktreeCommitMessageInput: "pr-stack-dirty-worktree-commit-message-input",
  prStackDirtyWorktreeCommitBtn: "pr-stack-dirty-worktree-commit-btn",
  prStackDirtyWorktreeCancelBtn: "pr-stack-dirty-worktree-cancel-btn",
} as const;

export const prStackPlannedPrRow = (nodeId: string) => `pr-stack-planned-pr-row-${nodeId}`;

export const prStackStartSessionBtn = (nodeId: string) => `pr-stack-start-session-${nodeId}`;

export const prStackStatusChip = (nodeId: string) => `pr-stack-status-chip-${nodeId}`;

export const prStackInternalStatusBadge = (nodeId: string) =>
  `pr-stack-internal-status-badge-${nodeId}`;

export const prStackInProgressBadge = (nodeId: string) => `pr-stack-in-progress-${nodeId}`;

export const prStackPrLink = (nodeId: string) => `pr-stack-pr-link-${nodeId}`;

export const prStackPrState = (nodeId: string) => `pr-stack-pr-state-${nodeId}`;

export const prStackRepointBtn = (nodeId: string) => `pr-stack-repoint-${nodeId}`;

export const prStackWorktree = (nodeId: string) => `pr-stack-worktree-${nodeId}`;

export const prStackSession = (nodeId: string) => `pr-stack-session-${nodeId}`;

export const prStackRowToggle = (nodeId: string) => `pr-stack-row-toggle-${nodeId}`;

export const prStackRowDetails = (nodeId: string) => `pr-stack-row-details-${nodeId}`;

export const prStackNodeId = (nodeId: string) => `pr-stack-node-id-${nodeId}`;

export const prStackParents = (nodeId: string) => `pr-stack-parents-${nodeId}`;

export const prStackChildRecipe = (nodeId: string) => `pr-stack-child-recipe-${nodeId}`;

export const prStackChildSession = (nodeId: string) => `pr-stack-child-session-${nodeId}`;

export const prStackChildState = (nodeId: string) => `pr-stack-child-state-${nodeId}`;

export const prStackMoveUpBtn = (nodeId: string) => `pr-stack-move-up-${nodeId}`;

export const prStackMoveDownBtn = (nodeId: string) => `pr-stack-move-down-${nodeId}`;

export const prStackReorderError = (nodeId: string) => `pr-stack-reorder-error-${nodeId}`;

export const prStackBaseBehind = (nodeId: string) => `pr-stack-base-behind-${nodeId}`;

export const prStackBaseInSync = (nodeId: string) => `pr-stack-base-in-sync-${nodeId}`;

export const prStackBaseConflicts = (nodeId: string) => `pr-stack-base-conflicts-${nodeId}`;

export const prStackBaseSyncUnavailable = (nodeId: string) => `pr-stack-base-sync-unavailable-${nodeId}`;

export const prStackBaseConflictPaths = (nodeId: string) => `pr-stack-base-conflict-paths-${nodeId}`;

export const prStackSyncMergeBtn = (nodeId: string) => `pr-stack-sync-merge-${nodeId}`;

export const prStackSyncRebaseBtn = (nodeId: string) => `pr-stack-sync-rebase-${nodeId}`;

export const prStackSyncError = (nodeId: string) => `pr-stack-sync-error-${nodeId}`;

export const prStackBranch = (nodeId: string) => `pr-stack-branch-${nodeId}`;

export const prStackPlannedBranch = (nodeId: string) => `pr-stack-planned-branch-${nodeId}`;

export const prStackBaseBranch = (nodeId: string) => `pr-stack-base-branch-${nodeId}`;

export const prStackStartWarning = (nodeId: string) => `pr-stack-start-warning-${nodeId}`;

export const prStackStartSessionBlockedIcon = (nodeId: string) =>
  `pr-stack-start-session-blocked-icon-${nodeId}`;

export const prStackRepointError = (nodeId: string) => `pr-stack-repoint-error-${nodeId}`;

export const prStackPrUnavailable = (nodeId: string) => `pr-stack-pr-unavailable-${nodeId}`;

export const agentChatMessage = (index: number) => `agent-chat-message-${index}`;

export const agentChatOption = (index: number) => `agent-chat-option-${index}`;

export const agentChatMultiSelectOption = (index: number) => `agent-chat-multiselect-option-${index}`;

export const agentChatElapsed = (index: number) => `agent-chat-elapsed-${index}`;

export const agentChatToolStatus = (index: number) => `agent-chat-tool-status-${index}`;

export const prStackAddPlannedPrAncestorCheckbox = (nodeId: string) =>
  `pr-stack-add-planned-pr-ancestor-${nodeId}`;
