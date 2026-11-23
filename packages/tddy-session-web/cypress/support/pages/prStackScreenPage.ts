/**
 * Page object for the PR-Stack Chat Screen acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods.
 * No raw `cy.get(...)` in test files — only these named helpers.
 */

import {
  byTestId,
  prStackPlannedPrRow,
  prStackStartSessionBtn,
  prStackStatusChip,
  prStackInternalStatusBadge,
  agentChatMessage,
  agentChatOption,
  agentChatMultiSelectOption,
  prStackAddPlannedPrAncestorCheckbox,
  prStackInProgressBadge,
  prStackPrLink,
  prStackPrState,
  prStackRepointBtn,
  prStackWorktree,
  prStackSession,
  prStackBranch,
  prStackPlannedBranch,
  prStackBaseBranch,
  prStackStartWarning,
  prStackStartSessionBlockedIcon,
  prStackRepointError,
  prStackPrUnavailable,
  prStackRowToggle,
  prStackRowDetails,
  prStackNodeId,
  prStackParents,
  prStackChildRecipe,
  prStackChildSession,
  prStackChildState,
  prStackMoveUpBtn,
  prStackMoveDownBtn,
  prStackReorderError,
  prStackBaseBehind,
  prStackBaseInSync,
  prStackBaseConflicts,
  prStackBaseSyncUnavailable,
  prStackBaseConflictPaths,
  prStackSyncMergeBtn,
  prStackSyncRebaseBtn,
  prStackSyncError,
  TEST_IDS,
} from "../testIds";

/** The fields the "New planned PR" form collects. Only `title` is required by the form itself. */
export interface AddWorkItemFormFields {
  title: string;
  description?: string;
  branchSuggestion?: string;
  ancestorNodeIds?: string[];
}

export const prStackScreenPage = {
  // ---------------------------------------------------------------------------
  // Planned-PR list
  // ---------------------------------------------------------------------------

  /** The list container for planned PRs. */
  workItemList: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackPlannedPrList, { timeout: 5000, ...options }),

  /** A single planned-PR row for the given stack node id. */
  workItemRow: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackPlannedPrRow(itemId), { timeout: 5000, ...options }),

  /** The node ids of every rendered planned-PR row, in DOM order. */
  workItemRowNodeIds: (): Cypress.Chainable<string[]> =>
    prStackScreenPage
      .workItemList()
      .find("[data-testid^='work-item-row-']")
      .then(($rows) =>
        [...$rows].map((el) => el.getAttribute("data-testid")!.replace("work-item-row-", "")),
      ),

  /** The "Start session" CTA on an unspawned planned-PR row. */
  startSessionBtn: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackStartSessionBtn(itemId), { timeout: 5000, ...options }),

  /** The status chip on an already-spawned planned-PR row. */
  statusChip: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackStatusChip(itemId), { timeout: 5000, ...options }),

  /** The action-needed internal-status badge on a planned-PR row (e.g. "needs-repoint"). */
  internalStatusBadge: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackInternalStatusBadge(itemId), { timeout: 5000, ...options }),

  /** The in-progress indicator on a row whose branch is owned by a live session. */
  inProgressBadge: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackInProgressBadge(itemId), { timeout: 5000, ...options }),

  /** The GitHub PR number rendered as a link to the PR for a row. */
  prLink: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackPrLink(itemId), { timeout: 5000, ...options }),

  /** The GitHub PR state label (open/merged/closed/draft) for a row. */
  prState: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackPrState(itemId), { timeout: 5000, ...options }),

  /** The Repoint control on a row whose predecessor has merged. */
  repointBtn: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackRepointBtn(itemId), { timeout: 5000, ...options }),

  /** The on-disk worktree indicator for a row (resolved by branch via QueryBranch). */
  worktree: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackWorktree(itemId), { timeout: 5000, ...options }),

  /**
   * The control that opens the child session a row is bound to — the status chip made clickable.
   * Absent when neither the plan's recorded child nor the branch's owner is a known session.
   */
  sessionRef: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackSession(itemId), { timeout: 5000, ...options }),

  /** Open the child session a row is bound to. */
  openBoundSession(itemId: string) {
    byTestId(prStackSession(itemId)).click();
  },

  // ---------------------------------------------------------------------------
  // Row expansion
  // ---------------------------------------------------------------------------

  /** A row's header control — clicking it expands or collapses that row's detail. */
  rowToggle: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackRowToggle(itemId), { timeout: 5000, ...options }),

  /** A row's detail body. Always mounted; hidden rather than unmounted while collapsed. */
  rowDetails: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackRowDetails(itemId), { timeout: 5000, ...options }),

  /** The planner-assigned node id, shown in the row's detail. */
  nodeIdLabel: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackNodeId(itemId), { timeout: 5000, ...options }),

  /** The titles of the nodes this row is stacked on, shown in its detail. */
  parents: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackParents(itemId), { timeout: 5000, ...options }),

  /** The recipe the row's child session runs, shown in its detail. */
  childRecipe: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackChildRecipe(itemId), { timeout: 5000, ...options }),

  /** The bound child session's id, shown in the row's detail. */
  childSession: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackChildSession(itemId), { timeout: 5000, ...options }),

  /** The child session's last-recorded workflow state, shown in the row's detail. */
  childState: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackChildState(itemId), { timeout: 5000, ...options }),

  /** Expand a row to reveal its detail. */
  expandRow(itemId: string) {
    byTestId(prStackRowToggle(itemId)).click();
  },

  /**
   * Assert the row header names the detail body it reveals: its `aria-controls` must resolve to that
   * row's own detail container, rather than merely being present.
   */
  expectRowToggleControlsDetails(itemId: string) {
    prStackScreenPage
      .rowToggle(itemId)
      .invoke("attr", "aria-controls")
      .should("be.a", "string")
      .then((controlsId) => {
        cy.get(`#${controlsId}`).should("have.attr", "data-testid", prStackRowDetails(itemId));
      });
  },

  // ---------------------------------------------------------------------------
  // Row order
  // ---------------------------------------------------------------------------

  /** Move a row one position earlier in the persisted order. */
  moveUpBtn: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackMoveUpBtn(itemId), { timeout: 5000, ...options }),

  /** Move a row one position later in the persisted order. */
  moveDownBtn: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackMoveDownBtn(itemId), { timeout: 5000, ...options }),

  /** Click a row's move-up control. */
  clickMoveUp(itemId: string) {
    byTestId(prStackMoveUpBtn(itemId)).click();
  },

  /** Click a row's move-down control. */
  clickMoveDown(itemId: string) {
    byTestId(prStackMoveDownBtn(itemId)).click();
  },

  /** The daemon's reason for refusing or failing a reorder, shown outside the collapse boundary. */
  reorderError: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackReorderError(itemId), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Standing against the base branch
  // ---------------------------------------------------------------------------

  /** How many commits the row's branch is behind its base. */
  baseBehind: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackBaseBehind(itemId), { timeout: 5000, ...options }),

  /** The row's branch contains every commit on its base. */
  baseInSync: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackBaseInSync(itemId), { timeout: 5000, ...options }),

  /** Merging the base into the row's branch would conflict. */
  baseConflicts: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackBaseConflicts(itemId), { timeout: 5000, ...options }),

  /** The base comparison could not be made — distinct from a clean result. */
  baseSyncUnavailable: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackBaseSyncUnavailable(itemId), { timeout: 5000, ...options }),

  /** The conflicting paths, in the row's detail. */
  baseConflictPaths: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackBaseConflictPaths(itemId), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Pulling the base into the row's branch
  // ---------------------------------------------------------------------------

  /** Pull the base in by merging it — the default strategy. */
  syncMergeBtn: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackSyncMergeBtn(itemId), { timeout: 5000, ...options }),

  /** Pull the base in by rebasing the branch onto it. */
  syncRebaseBtn: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackSyncRebaseBtn(itemId), { timeout: 5000, ...options }),

  /** The daemon's reason for refusing or failing a pull, shown outside the collapse boundary. */
  syncError: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackSyncError(itemId), { timeout: 5000, ...options }),

  /** Click a row's merge-from-base control. */
  clickSyncMerge(itemId: string) {
    byTestId(prStackSyncMergeBtn(itemId)).click();
  },

  /** Click a row's rebase-onto-base control. */
  clickSyncRebase(itemId: string) {
    byTestId(prStackSyncRebaseBtn(itemId)).click();
  },

  /** The prompt shown when a pull targets a worktree holding uncommitted work. */
  dirtyWorktreeDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackDirtyWorktreeDialog, { timeout: 5000, ...options }),

  /** The uncommitted paths the prompt names. */
  dirtyWorktreePaths: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackDirtyWorktreePaths, { timeout: 5000, ...options }),

  /** The commit message the outstanding changes would be committed with. */
  dirtyWorktreeCommitMessageInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackDirtyWorktreeCommitMessageInput, { timeout: 5000, ...options }),

  /** Commit and push the outstanding changes, then continue with the pull. */
  dirtyWorktreeCommitBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackDirtyWorktreeCommitBtn, { timeout: 5000, ...options }),

  /** Abandon the pull, leaving the worktree untouched. */
  dirtyWorktreeCancelBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackDirtyWorktreeCancelBtn, { timeout: 5000, ...options }),

  /** Confirm the dirty-worktree prompt: commit the outstanding work with `message`, then pull. */
  commitDirtyWorktreeAndPull(message: string) {
    byTestId(TEST_IDS.prStackDirtyWorktreeCommitMessageInput).clear().type(message);
    byTestId(TEST_IDS.prStackDirtyWorktreeCommitBtn).click();
  },

  /** The branch a row's planned PR owns — a branch that exists. */
  branchName: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackBranch(itemId), { timeout: 5000, ...options }),

  /** The row's planned branch name (`branch_suggestion`), rendered distinctly from an owned branch. */
  plannedBranchName: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackPlannedBranch(itemId), { timeout: 5000, ...options }),

  /** The base branch a row's child worktree would be branched from. */
  baseBranch: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackBaseBranch(itemId), { timeout: 5000, ...options }),

  /** The warning listing every reason a row's spawn may not succeed — shown beside its marked CTA. */
  startWarning: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackStartWarning(itemId), { timeout: 5000, ...options }),

  /**
   * The alert icon on a blocked row's Start-session button. The button itself stays pressable — a
   * blocker is advice, not a refusal — so this icon is what marks the difference.
   */
  startSessionBlockedIcon: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackStartSessionBlockedIcon(itemId), { timeout: 5000, ...options }),

  /** The inline error shown on a row whose repoint the daemon refused. */
  repointError: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackRepointError(itemId), { timeout: 5000, ...options }),

  /** The "PR status unavailable" indicator — the lookup could not be performed. */
  prUnavailable: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackPrUnavailable(itemId), { timeout: 5000, ...options }),

  /** Click the Repoint control for a row. */
  clickRepoint(itemId: string) {
    byTestId(prStackRepointBtn(itemId)).click();
  },

  // ---------------------------------------------------------------------------
  // Manually adding a planned PR (deterministic, non-chat path)
  // ---------------------------------------------------------------------------

  /** The "+ New planned PR" entry-point button that opens the add-planned-PR form. */
  addWorkItemBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrBtn, { timeout: 5000, ...options }),

  /** The add-planned-PR form root. */
  addWorkItemForm: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrForm, { timeout: 5000, ...options }),

  /** The new planned PR's title input. */
  addWorkItemTitleInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrTitleInput, { timeout: 5000, ...options }),

  /** The new planned PR's description input. */
  addWorkItemDescriptionInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrDescriptionInput, { timeout: 5000, ...options }),

  /** The new planned PR's optional branch-suggestion input. */
  addWorkItemBranchSuggestionInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrBranchSuggestionInput, { timeout: 5000, ...options }),

  /** An ancestor checkbox for the given existing planned-PR node id. */
  addWorkItemAncestorCheckbox: (itemId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackAddPlannedPrAncestorCheckbox(itemId), { timeout: 5000, ...options }),

  /** Submit button for the add-planned-PR form. */
  addWorkItemSubmitBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrSubmitBtn, { timeout: 5000, ...options }),

  /** Cancel button for the add-planned-PR form. */
  addWorkItemCancelBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrCancelBtn, { timeout: 5000, ...options }),

  /** Inline error banner shown when adding a planned PR fails. */
  addWorkItemError: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrError, { timeout: 5000, ...options }),

  /** Open the "New planned PR" form. */
  openAddPlannedPrForm() {
    byTestId(TEST_IDS.prStackAddPlannedPrBtn).click();
  },

  /**
   * Fill the "New planned PR" form's fields *without* submitting: types the title (and optional
   * description / branch suggestion) and checks the given ancestor node ids. Assumes the form is
   * already open.
   *
   * The form has two terminal actions ("Add planned PR" and "Add & start session") that differ only
   * in the button they click, so the filling lives here once and each action is a thin wrapper.
   */
  fillAddPlannedPrForm(options: AddWorkItemFormFields) {
    byTestId(TEST_IDS.prStackAddPlannedPrTitleInput).clear().type(options.title);
    if (options.description) {
      byTestId(TEST_IDS.prStackAddPlannedPrDescriptionInput).clear().type(options.description);
    }
    if (options.branchSuggestion) {
      byTestId(TEST_IDS.prStackAddPlannedPrBranchSuggestionInput)
        .clear()
        .type(options.branchSuggestion);
    }
    for (const itemId of options.ancestorNodeIds ?? []) {
      byTestId(prStackAddPlannedPrAncestorCheckbox(itemId)).click();
    }
  },

  /**
   * Fill the "New planned PR" form and submit it through "Add planned PR", which appends the node and
   * leaves it planned for later.
   */
  fillAndSubmitAddPlannedPrForm(options: AddWorkItemFormFields) {
    prStackScreenPage.fillAddPlannedPrForm(options);
    byTestId(TEST_IDS.prStackAddPlannedPrSubmitBtn).click();
  },

  /**
   * Fill the "New planned PR" form and submit it through "Add & start session", which adds the node
   * and opens the Start-session dialog for it in one step. Same fields as
   * `fillAndSubmitAddPlannedPrForm`, different terminal action — so a test picks the action it means
   * rather than passing a flag.
   */
  fillAddPlannedPrFormAndStartSession(options: AddWorkItemFormFields) {
    prStackScreenPage.fillAddPlannedPrForm(options);
    byTestId(TEST_IDS.prStackAddPlannedPrStartBtn).click();
  },

  /** The "Add & start session" action on the add-planned-PR form. */
  addWorkItemStartBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrStartBtn, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Chat window
  // ---------------------------------------------------------------------------

  /** The chat panel root. */
  chat: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChat, { timeout: 5000, ...options }),

  /** The scrollable message list. */
  chatMessages: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatMessages, { timeout: 5000, ...options }),

  /** A single rendered chat bubble, in arrival order (0-indexed). */
  chatMessage: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentChatMessage(index), { timeout: 5000, ...options }),

  /** The bubble kind ("user" | "agent" | "goal" | "activity") of a chat bubble, in arrival order. */
  chatMessageKind: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentChatMessage(index), { timeout: 5000, ...options }).invoke("attr", "data-message-kind"),

  /** The chat text input. */
  chatInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatInput, { timeout: 5000, ...options }),

  /** The chat send button. */
  chatSendBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatSendBtn, { timeout: 5000, ...options }),

  /** The chat's inline error banner — connection failures, stream failures, or a rejected send. */
  chatError: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatError, { timeout: 5000, ...options }),

  /** The overlay shown while the presenter's own LiveKit room is still connecting. */
  chatConnectingOverlay: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatConnecting, { timeout: 5000, ...options }),

  /** The persistent presenter connection status label (Not connected / Connecting… / Connected / Disconnected). */
  chatStatus: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatStatus, { timeout: 5000, ...options }),

  /** Type a message into the chat input and click Send. */
  sendChatMessage(text: string) {
    byTestId(TEST_IDS.agentChatInput).clear().type(text);
    byTestId(TEST_IDS.agentChatSendBtn).click();
  },

  // ---------------------------------------------------------------------------
  // Clarification question elicitation (AppMode::Select / MultiSelect)
  // ---------------------------------------------------------------------------

  /** The clarification-question panel root, shown while the workflow awaits an answer. */
  chatQuestion: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatQuestion, { timeout: 5000, ...options }),

  /** The question's header (short category label, e.g. "Backend"). */
  chatQuestionHeader: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatQuestionHeader, { timeout: 5000, ...options }),

  /** The question's full text. */
  chatQuestionText: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatQuestionText, { timeout: 5000, ...options }),

  /** A single-select option button, in option order (0-indexed). Clicking answers immediately. */
  chatOption: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentChatOption(index), { timeout: 5000, ...options }),

  /** A multi-select option checkbox, in option order (0-indexed). Toggling does not answer immediately. */
  chatMultiSelectOption: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentChatMultiSelectOption(index), { timeout: 5000, ...options }),

  /** Toggle a multi-select checkbox on. */
  toggleMultiSelectOption(index: number) {
    prStackScreenPage.chatMultiSelectOption(index).click();
  },

  /** Submit the checked multi-select options (and optional "Other" text). */
  submitMultiSelect() {
    byTestId(TEST_IDS.agentChatMultiSelectSubmit).click();
  },

  /** The free-text "Other" input for a question that allows a custom answer. */
  chatQuestionOtherInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatQuestionOtherInput, { timeout: 5000, ...options }),

  /** Type into the "Other" input without submitting (multi-select: submitted via `submitMultiSelect`). */
  typeOtherText(text: string) {
    byTestId(TEST_IDS.agentChatQuestionOtherInput).clear().type(text);
  },

  /** Type and submit a custom "Other" answer for a single-select question. */
  answerOther(text: string) {
    prStackScreenPage.typeOtherText(text);
    byTestId(TEST_IDS.agentChatQuestionOtherSubmit).click();
  },

  // ---------------------------------------------------------------------------
  // Session-creation dialog (opened by the "Start session" CTA)
  // ---------------------------------------------------------------------------

  /** The overlay dialog wrapping the reused `CreateSessionPane`. */
  createSessionDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionDialog, { timeout: 5000, ...options }),

  /** The reused `CreateSessionPane` rendered inside the dialog. */
  createSessionPaneInDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionPane, { timeout: 5000, ...options }),

  /** The dialog's new-branch-name input (pre-filled from the planned PR's branch). */
  dialogNewBranchNameInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionNewBranchNameInput, { timeout: 5000, ...options }),

  /** The dialog's branch-mode select — its "New branch from base: <name>" option carries the base. */
  dialogBranchIntentSelect: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionBranchIntentSelect, { timeout: 5000, ...options }),

  /**
   * The label of the branch-mode select's selected `<option>` — the "New branch from base: <ref>"
   * caption that tells the operator which ref the child worktree will branch from. Yields it as a
   * string so a test can assert the base is shown (not blanked) for a blocked node.
   */
  dialogBranchIntentSelectedLabel: (): Cypress.Chainable<string> =>
    prStackScreenPage
      .dialogBranchIntentSelect()
      .find("option:selected")
      .invoke("text")
      .then((text) => String(text).trim()),

  /** The dialog's existing-branch select, shown in "Work on existing branch" mode. */
  dialogBranchToWorkOnSelect: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionBranchToWorkOnSelect, { timeout: 5000, ...options }),

  /** The dialog's base-branch <select> for a planned-PR child session — lists the node's direct
   *  dependency branches first (ordered by dependency depth), then the stack's other materialized
   *  branches. The selected value is sent as `selected_integration_base_ref`. */
  dialogBaseBranchSelect: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionBaseBranchSelect, { timeout: 5000, ...options }),

  /**
   * The base-branch `<select>`'s option values in render order — the refs the dialog can submit. Yields
   * a plain array so a test states the whole offered list, and its order, in one assertion.
   */
  dialogBaseBranchOptionValues: (options?: Parameters<typeof cy.get>[1]) =>
    prStackScreenPage
      .dialogBaseBranchSelect(options)
      .find("option")
      .then(($opts) => [...$opts].map((option) => (option as HTMLOptionElement).value)),

  /** The dialog's "Create Remote Branch" toggle — pre-checked for a stack node. */
  dialogCreateRemoteBranchToggle: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionCreateRemoteBranchToggle, { timeout: 5000, ...options }),

  /** The dialog's initial-prompt textarea (pre-filled from the planned PR's title + description). */
  dialogInitialPromptInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionInitialPromptInput, { timeout: 5000, ...options }),

  /** The dialog's Create button. */
  dialogSubmitBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionSubmitBtn, { timeout: 5000, ...options }),
};
