/**
 * Page object for attaching a roster agent from the session header and talking to it in a tab
 * (docs/ft/web/session-drawer.md § Add agent).
 *
 * The picker half is keyed by the agent's *qualified* id, for the reason `sessionAgentRosterPage`
 * gives: a bare name is ambiguous the moment two hosts offer one. The tab half is keyed by the
 * *conversation* id, because that is what the tab owns and what closing it cancels — an agent can be
 * attached with no conversation open, and the two ids answer different questions.
 */

import {
  TEST_IDS,
  agentConversationTurn,
  byTestId,
  sessionAgentPickerOption,
  sessionsAgentPane,
  sessionsAgentTab,
  sessionsAgentTabClose,
} from "../testIds";

export const sessionAgentConversationPage = {
  // --- attaching from the header -------------------------------------------
  /** The header's "Add agent" control. */
  attachBtn: () => byTestId(TEST_IDS.sessionAgentAttachBtn),

  openPicker() {
    byTestId(TEST_IDS.sessionAgentAttachBtn).click();
    byTestId(TEST_IDS.sessionAgentPicker).should("be.visible");
  },

  picker: () => byTestId(TEST_IDS.sessionAgentPicker),
  pickerOption: (agentId: string) => byTestId(sessionAgentPickerOption(agentId)),
  pickerWithdrawalWarning: () => byTestId(TEST_IDS.sessionAgentPickerWithdrawalWarning),
  attachError: () => byTestId(TEST_IDS.sessionAgentAttachError),

  /**
   * State the whole offer, in render order, rather than probing for the options a spec expects — an
   * existence check apiece passes against a picker that also offers an agent nobody named.
   *
   * Compared as *test ids* rather than as agent ids because the id is built through
   * `safeTestIdPart`, which is lossy (`explorer@local` → `explorer_local`). Building the expectation
   * with the same helper the component uses keeps that detail here instead of in the spec.
   */
  assertPickerOffers(agentIds: readonly string[]) {
    cy.get(`[data-testid^="${TEST_IDS.sessionAgentPicker}-option-"]`)
      .not(`[data-testid$="-host"]`)
      .should(($options) => {
        const rendered = $options.toArray().map((el) => el.getAttribute("data-testid"));
        expect(rendered).to.deep.equal(agentIds.map(sessionAgentPickerOption));
      });
  },

  selectInPicker(agentId: string) {
    byTestId(sessionAgentPickerOption(agentId)).click();
  },

  confirmAttach() {
    byTestId(TEST_IDS.sessionAgentPickerConfirmBtn).click();
  },

  /** Pick `agentId` out of the header picker and confirm the attach. */
  attachAgent(agentId: string) {
    this.openPicker();
    this.selectInPicker(agentId);
    this.confirmAttach();
  },

  // --- the conversation tab -------------------------------------------------
  tab: (conversationId: string) => byTestId(sessionsAgentTab(conversationId)),
  tabClose: (conversationId: string) => byTestId(sessionsAgentTabClose(conversationId)),
  pane: (conversationId: string) => byTestId(sessionsAgentPane(conversationId)),
  /** Every open agent tab, however many. Used to state "exactly one", which is the whole point of
   *  the re-attach case. */
  tabs: () => cy.get('[data-testid^="sessions-agent-tab-"]:not([data-testid$="-close"])'),

  /** Every mounted agent conversation body. The conversation id is minted by the browser, so a spec
   *  that never sent one cannot name the pane it expects — it can still say there is exactly one. */
  panes: () => cy.get('[data-testid^="sessions-agent-pane-"]'),

  closeTab(conversationId: string) {
    byTestId(sessionsAgentTabClose(conversationId)).click();
  },

  /**
   * Close the one open agent tab without naming it — for a spec that never learned the conversation
   * id because the daemon refused the open that would have recorded it.
   */
  closeOnlyTab() {
    cy.get('[data-testid^="sessions-agent-tab-"][data-testid$="-close"]').click();
  },

  // --- the transcript inside the tab ---------------------------------------
  transcript: () => byTestId(TEST_IDS.agentConversationTranscript),
  turn: (index: number) => byTestId(agentConversationTurn(index)),
  /** Every rendered turn — for stating how many exchanges the transcript holds. */
  turns: () => cy.get('[data-testid^="agent-conversation-turn-"]'),
  error: () => byTestId(TEST_IDS.agentConversationError),

  /** Type a prompt and send it with the button. */
  prompt(text: string) {
    byTestId(TEST_IDS.agentConversationInput).type(text);
    byTestId(TEST_IDS.agentConversationSendBtn).click();
  },

  /**
   * Type a prompt and send it with the Enter key — the other way in, and the one a `disabled` Send
   * button does not close.
   */
  promptWithEnter(text: string) {
    byTestId(TEST_IDS.agentConversationInput).type(`${text}{enter}`);
  },

  /**
   * Assert a turn's role and text together, so a failure says which turn was wrong rather than
   * which of two independent assertions tripped first.
   */
  assertTurn(index: number, role: "operator" | "agent", text: string) {
    byTestId(agentConversationTurn(index)).should("have.attr", "data-role", role);
    byTestId(agentConversationTurn(index)).should("have.text", text);
  },
};
