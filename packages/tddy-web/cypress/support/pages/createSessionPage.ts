/**
 * Page object for the CreateSessionPane (new-session form) acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods. No raw `cy.get(...)` in test files.
 */

import * as ids from "../testIds";
import { byTestId, TEST_IDS } from "../testIds";

export const createSessionPage = {
  // ---------------------------------------------------------------------------
  // Host selector (multi-daemon)
  // ---------------------------------------------------------------------------

  /** The "Host" `<select>` — which daemon/host runs the session. */
  hostSelect: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionHostSelect, { timeout: 5000, ...options }),

  /** The daemon-instance ids offered as host options, in option order. */
  hostOptionValues: (): Cypress.Chainable<string[]> =>
    createSessionPage
      .hostSelect()
      .find("option")
      .then(($opts) => [...$opts].map((el) => (el as HTMLOptionElement).value)),

  /** Choose which host runs the session. */
  selectHost(daemonInstanceId: string) {
    byTestId(TEST_IDS.createSessionHostSelect).select(daemonInstanceId);
  },

  // ---------------------------------------------------------------------------
  // Agent selector — the agent a tool session is started as, fanned out across hosts
  // ---------------------------------------------------------------------------

  /** The "Agent" `<select>`. Rendered for tool sessions only. */
  agentSelect: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionAgentSelect, { timeout: 5000, ...options }),

  /**
   * One offered agent, keyed by its option value — `{id}@{daemonInstanceId}` while the common room
   * advertises daemons, the bare id otherwise.
   */
  agentOption: (optionValue: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(ids.createSessionAgentSelectOption(optionValue), options),

  /** The option values the Agent `<select>` offers, in option order. */
  agentOptionValues: (): Cypress.Chainable<string[]> =>
    createSessionPage
      .agentSelect()
      .find("option")
      .then(($opts) => [...$opts].map((el) => (el as HTMLOptionElement).value)),

  /** The currently selected agent's option value. */
  selectedAgentValue: (): Cypress.Chainable<string> =>
    createSessionPage.agentSelect().then(($sel) => ($sel[0] as HTMLSelectElement).value),

  /** The placeholder shown when no host offered an agent. */
  agentEmptyOption: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionAgentEmptyOption, options),

  /** One error row for a host whose agent catalog could not be read. */
  agentHostError: (daemonInstanceId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(ids.createSessionAgentSelectHostError(daemonInstanceId), options),

  // ---------------------------------------------------------------------------
  // Codebase host — which daemon holds the worktree (docs/ft/daemon/remote-managed-worktree.md)
  // ---------------------------------------------------------------------------

  /** The "Codebase host" `<select>`. Present only inside an open claude-cli Managed codebase block. */
  codebaseHostSelect: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionCodebaseHostSelect, { timeout: 5000, ...options }),

  /** Choose which daemon's filesystem holds the worktree. `""` selects "same as host". */
  selectCodebaseHost(daemonInstanceId: string) {
    byTestId(TEST_IDS.createSessionCodebaseHostSelect).select(daemonInstanceId);
  },

  /**
   * The daemon-instance ids offered as codebase hosts, in option order. The leading "Same as host"
   * option carries the empty value, so it appears here as `""`.
   */
  codebaseHostOptionValues: (): Cypress.Chainable<string[]> =>
    createSessionPage
      .codebaseHostSelect()
      .find("option")
      .then(($opts) => [...$opts].map((el) => (el as HTMLOptionElement).value)),

  /** The captions rendered for the codebase-host options, in option order. */
  codebaseHostOptionLabels: (): Cypress.Chainable<string[]> =>
    createSessionPage
      .codebaseHostSelect()
      .find("option")
      .then(($opts) => [...$opts].map((el) => (el.textContent ?? "").trim())),

  /** The selector is not offered — split placement is unavailable in the current form state. */
  expectNoCodebaseHostSelector() {
    byTestId(TEST_IDS.createSessionCodebaseHostSelect).should("not.exist");
  },

  /** Open the "Managed codebase" section, which is where split placement is configured. */
  enableManagedCodebase() {
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).check();
  },

  /**
   * One specialized-agent checkbox inside an open Managed codebase section, keyed by the qualified
   * id (`<name>@<host>`) the form submits.
   */
  specializedAgentOption: (agentId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(ids.createSessionAgentOption(agentId), options),

  /** Attach a specialized agent to the session being created. */
  selectSpecializedAgent(agentId: string) {
    byTestId(ids.createSessionAgentOption(agentId)).click();
  },

  /** No specialized agent can be attached in the current form state. */
  expectNoSpecializedAgentPicker() {
    byTestId(TEST_IDS.createSessionManagedCodebaseSection).should("not.exist");
  },

  /** Close the "Managed codebase" section. */
  disableManagedCodebase() {
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).uncheck();
  },

  /** Switch the form to a Cursor CLI session. */
  switchToCursorCliSession() {
    byTestId(TEST_IDS.createSessionTypeCursorCliBtn).click();
  },

  // ---------------------------------------------------------------------------
  // Core fields
  // ---------------------------------------------------------------------------

  /** Choose the project. */
  selectProject(projectId: string) {
    byTestId(TEST_IDS.createSessionProjectSelect).select(projectId);
  },

  /** The "Project" `<select>` — which registry project the session is created in. */
  projectSelect: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionProjectSelect, { timeout: 5000, ...options }),

  /**
   * Wait until the asynchronously loaded project list has rendered, by which point every option the
   * form will offer is present. Lets a test assert on the whole option list without racing the
   * `ListProjects` round trip.
   */
  awaitProjectOption(projectId: string) {
    createSessionPage.projectSelect().find(`option[value='${projectId}']`).should("exist");
  },

  /**
   * The project ids offered as options, in option order. The leading "Select a project…" placeholder
   * is `disabled` and excluded — these are the projects a session can actually be created in.
   */
  projectOptionValues: (): Cypress.Chainable<string[]> =>
    createSessionPage
      .projectSelect()
      .find("option:not([disabled])")
      .then(($opts) => [...$opts].map((el) => (el as HTMLOptionElement).value)),

  /** The captions rendered for the selectable project options, in option order. */
  projectOptionLabels: (): Cypress.Chainable<string[]> =>
    createSessionPage
      .projectSelect()
      .find("option:not([disabled])")
      .then(($opts) => [...$opts].map((el) => (el.textContent ?? "").trim())),

  /**
   * Start the session as the agent carrying `optionValue` — `{id}@{daemonInstanceId}` while the
   * common room advertises daemons, the bare id otherwise.
   */
  selectAgent(optionValue: string) {
    byTestId(TEST_IDS.createSessionAgentSelect).select(optionValue);
  },

  /** Choose the workflow recipe (tool sessions). */
  selectRecipe(recipe: string) {
    byTestId(TEST_IDS.createSessionRecipeSelect).select(recipe);
  },

  /** The workflow-recipe `<select>` — for asserting it is offered at all. */
  recipeSelect: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionRecipeSelect, { timeout: 5000, ...options }),

  /** The recipe control is not offered — a recipe needs a repository the session does not have. */
  expectNoRecipeSelector() {
    byTestId(TEST_IDS.createSessionRecipeSelect).should("not.exist");
  },

  /** The "--dangerously-skip-permissions" checkbox — bypasses the agent's permission prompts. */
  dangerouslySkipPermissionsToggle: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionDangerouslySkipPermissionsToggle, {
      timeout: 5000,
      ...options,
    }),

  /** The "Sandbox" checkbox — jails the agent; resolves a worktree on the session's own daemon. */
  sandboxToggle: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionSandboxToggle, { timeout: 5000, ...options }),

  /** The "Semantic index" checkbox — indexes a worktree on the session's own daemon before launch. */
  semanticIndexToggle: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionSemanticIndexToggle, { timeout: 5000, ...options }),

  /** Switch the form to a Claude CLI session. */
  switchToClaudeCliSession() {
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
  },

  /** Switch the branch mode to "work on an existing branch", which triggers branch listing. */
  switchToWorkOnExistingBranch() {
    byTestId(TEST_IDS.createSessionBranchIntentSelect).select("work_on_selected_branch");
  },

  // ---------------------------------------------------------------------------
  // PR-stack base session — seeds a new orchestrator's stack with one existing session
  // ---------------------------------------------------------------------------

  /** The "Base the stack on" <select>. Present only for a tool session whose recipe is pr-stack. */
  prStackBaseSessionSelect: () => byTestId(TEST_IDS.createSessionPrStackBaseSessionSelect),

  /** The picker is offered — the gate is the recipe, not the branch mode. */
  expectPrStackBaseSessionPickerOffered() {
    byTestId(TEST_IDS.createSessionPrStackBaseSessionSelect).should("be.visible");
  },

  /** The picker is absent, for the recipes and session types that cannot seed a stack. */
  expectNoPrStackBaseSessionPicker() {
    byTestId(TEST_IDS.createSessionPrStackBaseSessionSelect).should("not.exist");
  },

  /**
   * The session ids offered as stack bases, in option order — the default option's value is "".
   *
   * Asserted rather than yielded, because the options are drawn from `ListSessions`: until that
   * answer lands the picker holds its default alone, and reading the options into a plain array
   * settles on whichever list happened to be mounted first instead of retrying until the offered one
   * is there.
   */
  expectPrStackBaseSessionOptionValues(sessionIds: string[]) {
    createSessionPage
      .prStackBaseSessionSelect()
      .find("option")
      .should(($opts) => {
        expect([...$opts].map((el) => (el as HTMLOptionElement).value)).to.deep.equal(sessionIds);
      });
  },

  /**
   * The stack-base options' visible labels, in option order — what the operator actually reads when
   * picking a base. States the whole offered list, and its order, in one assertion, and retries for
   * the same reason as {@link expectPrStackBaseSessionOptionValues}.
   */
  expectPrStackBaseSessionOptionLabels(labels: string[]) {
    createSessionPage
      .prStackBaseSessionSelect()
      .find("option")
      .should(($opts) => {
        expect([...$opts].map((el) => el.textContent ?? "")).to.deep.equal(labels);
      });
  },

  /** Choose the existing session whose branch seeds the new orchestrator's stack. */
  selectPrStackBaseSession(sessionId: string) {
    byTestId(TEST_IDS.createSessionPrStackBaseSessionSelect).select(sessionId);
  },

  /** Name the branch to create in "new branch from base" mode. */
  typeNewBranchName(branch: string) {
    byTestId(TEST_IDS.createSessionNewBranchNameInput).clear().type(branch);
  },

  /** The new-branch-name input itself — for asserting what a caller pre-filled it with. */
  newBranchNameInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionNewBranchNameInput, { timeout: 5000, ...options }),

  /** Submit the new-session form. */
  submit() {
    byTestId(TEST_IDS.createSessionSubmitBtn).click();
  },

  /** The Create button itself — for asserting it is disabled while a creation is in flight. */
  submitButton: () => byTestId(TEST_IDS.createSessionSubmitBtn),

  /** The form-level error strip, shown when the daemon refuses the creation. */
  error: () => byTestId(TEST_IDS.createSessionError),

  // ---------------------------------------------------------------------------
  // Attachments (docs/ft/coder/session-attachments.md)
  // ---------------------------------------------------------------------------

  /** The attachments section — also the drop target for an OS file drag. */
  attachmentsSection: () => byTestId(TEST_IDS.createSessionAttachmentsSection),

  /** Selector for the drop target, for the drag/drop helpers in `support/util/fileDrop`. */
  attachmentDropSelector: `[data-testid='${TEST_IDS.createSessionAttachmentsSection}']`,

  /** The drag-over overlay, shown only while a file drag is over the section. */
  attachmentDropOverlay: () => byTestId(TEST_IDS.createSessionAttachmentDropOverlay),

  /** The hidden native file input behind the "Attach files" label. */
  attachmentFileInput: () =>
    byTestId(TEST_IDS.createSessionAttachmentPickBtn).find("input[type='file']"),

  /** Pick local files through the native input. `force` is required: the input is `opacity-0`. */
  pickFiles(files: Cypress.FileReferenceObject[]) {
    createSessionPage.attachmentFileInput().selectFile(files, { force: true });
  },

  /** The inline refusal shown when a picked file cannot be attached. */
  attachmentError: () => byTestId(TEST_IDS.createSessionAttachmentError),

  /** An attachment row, keyed by the basename it will be materialized under. */
  attachmentRow: (basename: string) => byTestId(ids.createSessionAttachmentRow(basename)),

  /** Every attachment row's basename, in render order. */
  attachmentBasenames: (): Cypress.Chainable<string[]> =>
    createSessionPage
      .attachmentsSection()
      .find(`[data-attachment-basename]`)
      .then(($rows) => [...$rows].map((el) => el.getAttribute("data-attachment-basename") ?? "")),

  /**
   * Nothing is attached. Stated as its own absence assertion because {@link attachmentBasenames}
   * cannot express it: it reads the rows out of the section, and a form with none renders no row
   * elements to read.
   */
  expectNoAttachments() {
    createSessionPage.attachmentsSection().find(`[data-attachment-basename]`).should("not.exist");
  },

  /** An attachment row's rendered size in bytes. */
  attachmentSize: (basename: string) => byTestId(ids.createSessionAttachmentSize(basename)),

  /** Rename an attachment. Only `SessionAttachment.basename` changes; the source is untouched. */
  renameAttachment(from: string, to: string) {
    byTestId(ids.createSessionAttachmentName(from)).clear().type(to);
  },

  /** Drop an attachment before the session is created. */
  removeAttachment(basename: string) {
    byTestId(ids.createSessionAttachmentRemove(basename)).click();
  },

  /** An attachment row's progress element, carrying `data-attachment-percent`. */
  attachmentProgress: (basename: string) =>
    byTestId(ids.createSessionAttachmentProgress(basename)),

  // ---------------------------------------------------------------------------
  // Host-document picker — attaches by reference, with no upload
  // ---------------------------------------------------------------------------

  /** Open the picker for documents that already exist on the selected host. */
  openHostDocPicker() {
    byTestId(TEST_IDS.createSessionAttachmentPickHostDocBtn).click();
  },

  hostDocPicker: () => byTestId(TEST_IDS.createSessionHostDocPicker),

  /** Choose which `HostDocumentScope` the picker browses. */
  selectHostDocScope(scope: string) {
    byTestId(TEST_IDS.createSessionHostDocPickerScopeSelect).select(scope);
  },

  /** A listed document row in the picker, keyed by the `relative_path` it would reference. */
  hostDocRow: (relativePath: string) => byTestId(ids.createSessionHostDocRow(relativePath)),

  /** Attach the listed document at `relativePath`. */
  pickHostDoc(relativePath: string) {
    byTestId(ids.createSessionHostDocRow(relativePath)).click();
  },

  /**
   * A node in the picker's file tree. The worktree and project-repo scopes browse a real directory
   * tree, so they reuse `WorktreeFileTree` (and its node ids) rather than the flat rows the
   * artifact and upload scopes use.
   */
  hostDocTreeNode: (relPath: string) => byTestId(ids.worktreeTreeNode(relPath)),

  /** Expand a directory in the picker's tree, which lazily lists that directory. */
  expandHostDocDir(relPath: string) {
    byTestId(ids.worktreeTreeNode(relPath)).click();
  },

  /** Attach the tree file at `relPath`. */
  pickHostDocFromTree(relPath: string) {
    byTestId(ids.worktreeTreeNode(relPath)).click();
  },
};
