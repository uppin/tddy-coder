/**
 * Page object for the Agent roster pane and the fanned-out agent picker
 * (docs/ft/daemon/session-agent-roster.md § Web UI).
 *
 * Every lookup is keyed by the agent's *qualified* id, because a bare name is ambiguous the moment
 * two hosts offer one — which is the situation this feature exists to handle.
 */

import {
  TEST_IDS,
  agentTreeSession,
  agentTreeSessionChildren,
  agentTreeSessionRosterError,
  agentTreeSessionLastActivity,
  agentTreeSessionStatus,
  agentTreeSessionSwitchBtn,
  agentTreeSessionToggleBtn,
  agentRosterPickerHostError,
  agentRosterPickerOption,
  agentRosterPickerOptionHost,
  agentRosterRow,
  agentRosterRowCloneState,
  agentRosterRowDetachBtn,
  agentRosterRowHost,
  agentRosterRowLastActivity,
  agentRosterRowReplaces,
  agentRosterRowStatus,
  byTestId,
} from "../testIds";

export const sessionAgentRosterPage = {
  // --- reaching the pane from the Session Inspector -------------------------
  /** Open the Inspector's Agents tab, which is where this pane lives inside the drawer. */
  openInspectorAgentsTab() {
    byTestId(TEST_IDS.sessionInspectorTabAgents).click();
  },

  // --- states ---------------------------------------------------------------
  pane: () => byTestId(TEST_IDS.agentRosterPane),
  loading: () => byTestId(TEST_IDS.agentRosterLoading),
  disconnected: () => byTestId(TEST_IDS.agentRosterDisconnected),
  error: () => byTestId(TEST_IDS.agentRosterError),
  empty: () => byTestId(TEST_IDS.agentRosterEmpty),

  // --- attached agents ------------------------------------------------------
  row: (agentId: string) => byTestId(agentRosterRow(agentId)),
  rowHost: (agentId: string) => byTestId(agentRosterRowHost(agentId)),
  rowReplaces: (agentId: string) => byTestId(agentRosterRowReplaces(agentId)),
  rowCloneState: (agentId: string) => byTestId(agentRosterRowCloneState(agentId)),
  /** What the agent is doing. Always present — `unknown` is a value, not an absence. */
  rowStatus: (agentId: string) => byTestId(agentRosterRowStatus(agentId)),
  /** The agent's last observed activity. Absent when nothing has been observed of it. */
  rowLastActivity: (agentId: string) => byTestId(agentRosterRowLastActivity(agentId)),

  /**
   * Assert a row's status by its stable token rather than its prose, so rewording the badge is not
   * a test change. The prose is asserted once, where the wording itself is the point.
   */
  assertStatus(agentId: string, token: string) {
    byTestId(agentRosterRowStatus(agentId)).should("have.attr", "data-agent-status", token);
  },

  /** Click a row's detach action. Does not confirm — a remote agent's detach asks first. */
  clickDetach(agentId: string) {
    byTestId(agentRosterRowDetachBtn(agentId)).click();
  },

  detachConfirmation: () => byTestId(TEST_IDS.agentRosterDetachConfirm),

  confirmDetach() {
    byTestId(TEST_IDS.agentRosterDetachConfirmBtn).click();
  },

  // --- the tree -------------------------------------------------------------
  // The Agents tab is a hierarchy: the session's main agent at the root, its roster agents and its
  // subagent sessions beneath it. Containment is asserted by scoping into a parent's *children*
  // list, never by two independent existence checks — a flat list satisfies those.

  tree: () => byTestId(TEST_IDS.agentTree),
  rootRow: () => byTestId(TEST_IDS.agentTreeRoot),
  rootStatus: () => byTestId(TEST_IDS.agentTreeRootStatus),
  /** The main agent's last observed activity. Absent when nothing has been observed of it. */
  rootLastActivity: () => byTestId(TEST_IDS.agentTreeRootLastActivity),

  /** One subagent session's row, wherever in the tree it sits. */
  subagentRow: (sessionId: string) => byTestId(agentTreeSession(sessionId)),
  subagentStatus: (sessionId: string) => byTestId(agentTreeSessionStatus(sessionId)),
  subagentLastActivity: (sessionId: string) => byTestId(agentTreeSessionLastActivity(sessionId)),
  subagentSwitchBtn: (sessionId: string) => byTestId(agentTreeSessionSwitchBtn(sessionId)),

  /** Show a subagent session's own children — which is also what opens its roster stream. */
  expandSubagent(sessionId: string) {
    byTestId(agentTreeSessionToggleBtn(sessionId)).click();
  },

  /** Focus a subagent session's runtime. */
  clickSwitch(sessionId: string) {
    byTestId(agentTreeSessionSwitchBtn(sessionId)).click();
  },

  /** Why an expanded subagent's roster could not be read. Absent while it reads fine. */
  subagentRosterError: (sessionId: string) => byTestId(agentTreeSessionRosterError(sessionId)),

  /**
   * Assert a subagent session's row offers no detach. Scoped to the row rather than to the pane,
   * because a roster agent nested under the same subagent legitimately has one — the claim is about
   * this row, not about the subtree.
   */
  assertNoDetachOnSubagent(sessionId: string) {
    byTestId(agentTreeSession(sessionId)).find("[data-testid$='-detach-btn']").should("not.exist");
  },

  /** A roster agent's row must sit inside the main agent's children, not merely somewhere on screen. */
  assertRosterAgentUnderMainAgent(agentId: string) {
    byTestId(TEST_IDS.agentTreeRootChildren)
      .find(`[data-testid="${agentRosterRow(agentId)}"]`)
      .should("exist");
  },

  /** ...and a subagent session's row likewise. */
  assertSubagentUnderMainAgent(sessionId: string) {
    byTestId(TEST_IDS.agentTreeRootChildren)
      .find(`[data-testid="${agentTreeSession(sessionId)}"]`)
      .should("exist");
  },

  /** A roster agent attached to a *subagent* session belongs under that session, not under the root. */
  assertRosterAgentUnderSubagent(parentSessionId: string, agentId: string) {
    byTestId(agentTreeSessionChildren(parentSessionId))
      .find(`[data-testid="${agentRosterRow(agentId)}"]`)
      .should("exist");
  },

  /** A subagent's own subagent belongs under it. */
  assertSubagentUnderSubagent(parentSessionId: string, childSessionId: string) {
    byTestId(agentTreeSessionChildren(parentSessionId))
      .find(`[data-testid="${agentTreeSession(childSessionId)}"]`)
      .should("exist");
  },

  /**
   * Assert a subagent session's inferred status by its stable token, the same way a roster row's is
   * asserted — the two share one enum, so they must share one token.
   */
  assertSubagentStatus(sessionId: string, token: string) {
    byTestId(agentTreeSessionStatus(sessionId)).should("have.attr", "data-agent-status", token);
  },

  /** What the main agent itself is doing. */
  assertMainAgentStatus(token: string) {
    byTestId(TEST_IDS.agentTreeRootStatus).should("have.attr", "data-agent-status", token);
  },

  /** Whether a row is a roster agent this daemon manages, a session of its own, or the main agent. */
  assertRowKind(testId: string, kind: "main" | "roster" | "session") {
    byTestId(testId).should("have.attr", "data-agent-kind", kind);
  },

  /** How deep a row sits, so nesting is a fact in the DOM rather than a margin. */
  assertRowDepth(testId: string, depth: number) {
    byTestId(testId).should("have.attr", "data-depth", String(depth));
  },

  /**
   * Assert the session detail pane carries no "Session agents" section any more — neither its list
   * nor its empty state, since a pane that swapped one for the other still lists peers twice.
   *
   * Named here rather than in a page object of its own because the section has no page object left:
   * the tree is what replaced it, and this is the one assertion its removal needs.
   */
  assertNoLegacyPeerSection() {
    byTestId("session-agents-section").should("not.exist");
    byTestId("session-agents-empty").should("not.exist");
  },

  // --- the picker -----------------------------------------------------------
  openPicker() {
    byTestId(TEST_IDS.agentRosterAddBtn).click();
    byTestId(TEST_IDS.agentRosterPicker).should("be.visible");
  },

  pickerOption: (agentId: string) => byTestId(agentRosterPickerOption(agentId)),
  pickerOptionHost: (agentId: string) => byTestId(agentRosterPickerOptionHost(agentId)),
  pickerHostError: (daemonInstanceId: string) => byTestId(agentRosterPickerHostError(daemonInstanceId)),
  pickerWithdrawalWarning: () => byTestId(TEST_IDS.agentRosterPickerWithdrawalWarning),

  selectInPicker(agentId: string) {
    byTestId(agentRosterPickerOption(agentId)).click();
  },

  confirmAttach() {
    byTestId(TEST_IDS.agentRosterPickerConfirmBtn).click();
  },
};
