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
export interface AddPlannedPrFormFields {
  title: string;
  description?: string;
  branchSuggestion?: string;
  ancestorNodeIds?: string[];
}

export const prStackScreenPage = {
  // ---------------------------------------------------------------------------
  // Screen root
  // ---------------------------------------------------------------------------

  /** The PR-Stack Chat Screen root — rendered instead of the terminal for "pr-stack" sessions. */
  screen: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackScreen, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Planned-PR list
  // ---------------------------------------------------------------------------

  /** The list container for planned PRs. */
  plannedPrList: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackPlannedPrList, { timeout: 5000, ...options }),

  /** A single planned-PR row for the given stack node id. */
  plannedPrRow: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackPlannedPrRow(nodeId), { timeout: 5000, ...options }),

  /** The node ids of every rendered planned-PR row, in DOM order. */
  plannedPrRowNodeIds: (): Cypress.Chainable<string[]> =>
    prStackScreenPage
      .plannedPrList()
      .find("[data-testid^='pr-stack-planned-pr-row-']")
      .then(($rows) =>
        [...$rows].map((el) => el.getAttribute("data-testid")!.replace("pr-stack-planned-pr-row-", "")),
      ),

  /** The "Start session" CTA on an unspawned planned-PR row. */
  startSessionBtn: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackStartSessionBtn(nodeId), { timeout: 5000, ...options }),

  /** The status chip on an already-spawned planned-PR row. */
  statusChip: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackStatusChip(nodeId), { timeout: 5000, ...options }),

  /** The action-needed internal-status badge on a planned-PR row (e.g. "needs-repoint"). */
  internalStatusBadge: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackInternalStatusBadge(nodeId), { timeout: 5000, ...options }),

  /** The in-progress indicator on a row whose branch is owned by a live session. */
  inProgressBadge: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackInProgressBadge(nodeId), { timeout: 5000, ...options }),

  /** The GitHub PR number rendered as a link to the PR for a row. */
  prLink: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackPrLink(nodeId), { timeout: 5000, ...options }),

  /** The GitHub PR state label (open/merged/closed/draft) for a row. */
  prState: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackPrState(nodeId), { timeout: 5000, ...options }),

  /** The Repoint control on a row whose predecessor has merged. */
  repointBtn: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackRepointBtn(nodeId), { timeout: 5000, ...options }),

  /** The on-disk worktree indicator for a row (resolved by branch via QueryBranch). */
  worktree: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackWorktree(nodeId), { timeout: 5000, ...options }),

  /**
   * The control that opens the child session a row is bound to — the status chip made clickable.
   * Absent when neither the plan's recorded child nor the branch's owner is a known session.
   */
  sessionRef: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackSession(nodeId), { timeout: 5000, ...options }),

  /** Open the child session a row is bound to. */
  openBoundSession(nodeId: string) {
    byTestId(prStackSession(nodeId)).click();
  },

  // ---------------------------------------------------------------------------
  // Row expansion
  // ---------------------------------------------------------------------------

  /** A row's header control — clicking it expands or collapses that row's detail. */
  rowToggle: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackRowToggle(nodeId), { timeout: 5000, ...options }),

  /** A row's detail body. Always mounted; hidden rather than unmounted while collapsed. */
  rowDetails: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackRowDetails(nodeId), { timeout: 5000, ...options }),

  /** The planner-assigned node id, shown in the row's detail. */
  nodeIdLabel: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackNodeId(nodeId), { timeout: 5000, ...options }),

  /** The titles of the nodes this row is stacked on, shown in its detail. */
  parents: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackParents(nodeId), { timeout: 5000, ...options }),

  /** The recipe the row's child session runs, shown in its detail. */
  childRecipe: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackChildRecipe(nodeId), { timeout: 5000, ...options }),

  /** The bound child session's id, shown in the row's detail. */
  childSession: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackChildSession(nodeId), { timeout: 5000, ...options }),

  /** The child session's last-recorded workflow state, shown in the row's detail. */
  childState: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackChildState(nodeId), { timeout: 5000, ...options }),

  /** Expand a row to reveal its detail. */
  expandRow(nodeId: string) {
    byTestId(prStackRowToggle(nodeId)).click();
  },

  /**
   * Assert the row header names the detail body it reveals: its `aria-controls` must resolve to that
   * row's own detail container, rather than merely being present.
   */
  expectRowToggleControlsDetails(nodeId: string) {
    prStackScreenPage
      .rowToggle(nodeId)
      .invoke("attr", "aria-controls")
      .should("be.a", "string")
      .then((controlsId) => {
        cy.get(`#${controlsId}`).should("have.attr", "data-testid", prStackRowDetails(nodeId));
      });
  },

  // ---------------------------------------------------------------------------
  // Row order
  // ---------------------------------------------------------------------------

  /** Move a row one position earlier in the persisted order. */
  moveUpBtn: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackMoveUpBtn(nodeId), { timeout: 5000, ...options }),

  /** Move a row one position later in the persisted order. */
  moveDownBtn: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackMoveDownBtn(nodeId), { timeout: 5000, ...options }),

  /** Click a row's move-up control. */
  clickMoveUp(nodeId: string) {
    byTestId(prStackMoveUpBtn(nodeId)).click();
  },

  /** Click a row's move-down control. */
  clickMoveDown(nodeId: string) {
    byTestId(prStackMoveDownBtn(nodeId)).click();
  },

  /** The daemon's reason for refusing or failing a reorder, shown outside the collapse boundary. */
  reorderError: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackReorderError(nodeId), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Standing against the base branch
  // ---------------------------------------------------------------------------

  /** How many commits the row's branch is behind its base. */
  baseBehind: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackBaseBehind(nodeId), { timeout: 5000, ...options }),

  /** The row's branch contains every commit on its base. */
  baseInSync: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackBaseInSync(nodeId), { timeout: 5000, ...options }),

  /** Merging the base into the row's branch would conflict. */
  baseConflicts: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackBaseConflicts(nodeId), { timeout: 5000, ...options }),

  /** The base comparison could not be made — distinct from a clean result. */
  baseSyncUnavailable: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackBaseSyncUnavailable(nodeId), { timeout: 5000, ...options }),

  /** The conflicting paths, in the row's detail. */
  baseConflictPaths: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackBaseConflictPaths(nodeId), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Pulling the base into the row's branch
  // ---------------------------------------------------------------------------

  /** Pull the base in by merging it — the default strategy. */
  syncMergeBtn: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackSyncMergeBtn(nodeId), { timeout: 5000, ...options }),

  /** Pull the base in by rebasing the branch onto it. */
  syncRebaseBtn: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackSyncRebaseBtn(nodeId), { timeout: 5000, ...options }),

  /** The daemon's reason for refusing or failing a pull, shown outside the collapse boundary. */
  syncError: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackSyncError(nodeId), { timeout: 5000, ...options }),

  /** Click a row's merge-from-base control. */
  clickSyncMerge(nodeId: string) {
    byTestId(prStackSyncMergeBtn(nodeId)).click();
  },

  /** Click a row's rebase-onto-base control. */
  clickSyncRebase(nodeId: string) {
    byTestId(prStackSyncRebaseBtn(nodeId)).click();
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
  branchName: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackBranch(nodeId), { timeout: 5000, ...options }),

  /** The row's planned branch name (`branch_suggestion`), rendered distinctly from an owned branch. */
  plannedBranchName: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackPlannedBranch(nodeId), { timeout: 5000, ...options }),

  /** The base branch a row's child worktree would be branched from. */
  baseBranch: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackBaseBranch(nodeId), { timeout: 5000, ...options }),

  /** The warning listing every reason a row's spawn may not succeed — shown beside its marked CTA. */
  startWarning: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackStartWarning(nodeId), { timeout: 5000, ...options }),

  /**
   * The alert icon on a blocked row's Start-session button. The button itself stays pressable — a
   * blocker is advice, not a refusal — so this icon is what marks the difference.
   */
  startSessionBlockedIcon: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackStartSessionBlockedIcon(nodeId), { timeout: 5000, ...options }),

  /** The inline error shown on a row whose repoint the daemon refused. */
  repointError: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackRepointError(nodeId), { timeout: 5000, ...options }),

  /** The "PR status unavailable" indicator — the lookup could not be performed. */
  prUnavailable: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackPrUnavailable(nodeId), { timeout: 5000, ...options }),

  /** Click the Repoint control for a row. */
  clickRepoint(nodeId: string) {
    byTestId(prStackRepointBtn(nodeId)).click();
  },

  // ---------------------------------------------------------------------------
  // "Planned PRs" panel (right-side dock on desktop, full-screen overlay on mobile)
  // ---------------------------------------------------------------------------

  /** The Planned-PRs panel. Always mounted; read `data-state` for open/closed. */
  plannedPrPanel: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackPlannedPrPanel, { timeout: 5000, ...options }),

  /** The header control that opens/closes the Planned-PRs panel. */
  plannedPrPanelToggle: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackPlannedPrPanelToggle, { timeout: 5000, ...options }),

  /** The Planned-PRs panel's own close control. */
  plannedPrPanelClose: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackPlannedPrPanelClose, { timeout: 5000, ...options }),

  /** The planned-PR list as rendered *inside* the panel. */
  panelPlannedPrList: () =>
    prStackScreenPage.plannedPrPanel().find(`[data-testid="${TEST_IDS.prStackPlannedPrList}"]`),

  /** Open or close the Planned-PRs panel via its header control. */
  togglePlannedPrPanel() {
    byTestId(TEST_IDS.prStackPlannedPrPanelToggle).click();
  },

  /** Assert the panel spans the full width of the PR-Stack screen (the mobile overlay layout). */
  expectPanelSpansScreenWidth() {
    prStackScreenPage
      .screen()
      .invoke("outerWidth")
      .then((screenWidth) => {
        prStackScreenPage.plannedPrPanel().invoke("outerWidth").should("equal", screenWidth);
      });
  },

  /** Assert the panel is a narrower docked column beside the chat (the desktop layout). */
  expectPanelNarrowerThanScreen() {
    prStackScreenPage
      .screen()
      .invoke("outerWidth")
      .then((screenWidth) => {
        prStackScreenPage
          .plannedPrPanel()
          .invoke("outerWidth")
          .should("be.lessThan", screenWidth as number);
      });
  },

  // ---------------------------------------------------------------------------
  // Manually adding a planned PR (deterministic, non-chat path)
  // ---------------------------------------------------------------------------

  /** The "+ New planned PR" entry-point button that opens the add-planned-PR form. */
  addPlannedPrBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrBtn, { timeout: 5000, ...options }),

  /** The add-planned-PR form root. */
  addPlannedPrForm: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrForm, { timeout: 5000, ...options }),

  /** The new planned PR's title input. */
  addPlannedPrTitleInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrTitleInput, { timeout: 5000, ...options }),

  /** The new planned PR's description input. */
  addPlannedPrDescriptionInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrDescriptionInput, { timeout: 5000, ...options }),

  /** The new planned PR's optional branch-suggestion input. */
  addPlannedPrBranchSuggestionInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrBranchSuggestionInput, { timeout: 5000, ...options }),

  /** An ancestor checkbox for the given existing planned-PR node id. */
  addPlannedPrAncestorCheckbox: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackAddPlannedPrAncestorCheckbox(nodeId), { timeout: 5000, ...options }),

  /** Submit button for the add-planned-PR form. */
  addPlannedPrSubmitBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrSubmitBtn, { timeout: 5000, ...options }),

  /** Cancel button for the add-planned-PR form. */
  addPlannedPrCancelBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrCancelBtn, { timeout: 5000, ...options }),

  /** Inline error banner shown when adding a planned PR fails. */
  addPlannedPrError: (options?: Parameters<typeof cy.get>[1]) =>
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
  fillAddPlannedPrForm(options: AddPlannedPrFormFields) {
    byTestId(TEST_IDS.prStackAddPlannedPrTitleInput).clear().type(options.title);
    if (options.description) {
      byTestId(TEST_IDS.prStackAddPlannedPrDescriptionInput).clear().type(options.description);
    }
    if (options.branchSuggestion) {
      byTestId(TEST_IDS.prStackAddPlannedPrBranchSuggestionInput)
        .clear()
        .type(options.branchSuggestion);
    }
    for (const nodeId of options.ancestorNodeIds ?? []) {
      byTestId(prStackAddPlannedPrAncestorCheckbox(nodeId)).click();
    }
  },

  /**
   * Fill the "New planned PR" form and submit it through "Add planned PR", which appends the node and
   * leaves it planned for later.
   */
  fillAndSubmitAddPlannedPrForm(options: AddPlannedPrFormFields) {
    prStackScreenPage.fillAddPlannedPrForm(options);
    byTestId(TEST_IDS.prStackAddPlannedPrSubmitBtn).click();
  },

  /**
   * Fill the "New planned PR" form and submit it through "Add & start session", which adds the node
   * and opens the Start-session dialog for it in one step. Same fields as
   * `fillAndSubmitAddPlannedPrForm`, different terminal action — so a test picks the action it means
   * rather than passing a flag.
   */
  fillAddPlannedPrFormAndStartSession(options: AddPlannedPrFormFields) {
    prStackScreenPage.fillAddPlannedPrForm(options);
    byTestId(TEST_IDS.prStackAddPlannedPrStartBtn).click();
  },

  /** The "Add & start session" action on the add-planned-PR form. */
  addPlannedPrStartBtn: (options?: Parameters<typeof cy.get>[1]) =>
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

  /** The dialog's pre-checked "Create Remote Branch" toggle. */
  dialogCreateRemoteBranchToggle: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionCreateRemoteBranchToggle, { timeout: 5000, ...options }),

  /** The dialog's initial-prompt textarea (pre-filled from the planned PR's title + description). */
  dialogInitialPromptInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionInitialPromptInput, { timeout: 5000, ...options }),

  /** The dialog's Create button. */
  dialogSubmitBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionSubmitBtn, { timeout: 5000, ...options }),
};
