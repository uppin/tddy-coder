/**
 * Page object for the Agent roster pane and the fanned-out agent picker
 * (docs/ft/daemon/session-agent-roster.md § Web UI).
 *
 * Every lookup is keyed by the agent's *qualified* id, because a bare name is ambiguous the moment
 * two hosts offer one — which is the situation this feature exists to handle.
 */

import {
  TEST_IDS,
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
