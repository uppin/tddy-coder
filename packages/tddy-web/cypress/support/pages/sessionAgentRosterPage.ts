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
  agentRosterRowReplaces,
  byTestId,
} from "../testIds";

export const sessionAgentRosterPage = {
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
