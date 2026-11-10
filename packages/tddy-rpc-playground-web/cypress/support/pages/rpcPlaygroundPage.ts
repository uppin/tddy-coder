/**
 * Page object for the RPC Playground screen.
 *
 * All raw selectors live here; test bodies call named methods.
 * No raw `cy.get(...)` in test files — only these named helpers.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const rpcPlaygroundPage = {
  /** The service tree listing every reflected service. */
  serviceTree: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.rpcServiceTree, { timeout: 5000, ...options }),

  /** Expand a service row to reveal its methods. */
  expandService: (serviceName: string) => {
    byTestId(`rpc-service-${serviceName}`, { timeout: 5000 }).click();
  },

  /** Pick a method from an expanded service row. */
  chooseMethod: (serviceName: string, methodName: string) => {
    byTestId(`rpc-method-${serviceName}-${methodName}`, { timeout: 5000 }).click();
  },

  /** The request editor, present only once a method is selected. */
  requestEditor: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.rpcRequestEditor, { timeout: 5000, ...options }),

  /** Asserts the request editor is open and headed by `service/method`. */
  expectEditorFor: (serviceName: string, methodName: string) => {
    byTestId(TEST_IDS.rpcRequestEditor, { timeout: 5000 }).should(
      "contain.text",
      `${serviceName}/${methodName}`,
    );
  },

  /** Asserts no method is selected, so no request editor is rendered. */
  expectNoMethodSelected: () => {
    byTestId(TEST_IDS.rpcRequestEditor, { timeout: 5000 }).should("not.exist");
  },

  /** The participant/host picker, offered only on a connection that carries a roster. */
  participantSelect: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.rpcPlaygroundParticipantSelect, { timeout: 5000, ...options }),

  /** What replaces the picker on a connection that carries no LiveKit presence. */
  participantSelectionUnavailable: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.rpcPlaygroundParticipantUnavailable, { timeout: 5000, ...options }),

  /** Pick the participant (host) the playground addresses. */
  chooseParticipant: (participantId: string) => {
    byTestId(TEST_IDS.rpcPlaygroundParticipantSelect, { timeout: 5000 }).select(participantId);
  },
};
