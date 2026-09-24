/**
 * Page object for the credential vault prompt (`CredentialVaultPrompt`) — the passphrase a signed-in
 * operator gives when their vault is locked, or chooses when it has not been created yet.
 *
 * All raw selectors live here; test bodies call named methods.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const credentialVaultPromptPage = {
  expectAskingToUnlock() {
    byTestId(TEST_IDS.credentialVaultPrompt).should("be.visible").and("contain.text", "Unlock");
    byTestId(TEST_IDS.credentialVaultPassphraseConfirm).should("not.exist");
  },

  expectAskingToCreate() {
    byTestId(TEST_IDS.credentialVaultPrompt).should("be.visible").and("contain.text", "Create");
    byTestId(TEST_IDS.credentialVaultPassphraseConfirm).should("be.visible");
  },

  expectNoPrompt() {
    byTestId(TEST_IDS.credentialVaultPrompt).should("not.exist");
  },

  typePassphrase(passphrase: string) {
    byTestId(TEST_IDS.credentialVaultPassphrase).clear().type(passphrase, { log: false });
  },

  typeConfirmation(passphrase: string) {
    byTestId(TEST_IDS.credentialVaultPassphraseConfirm).clear().type(passphrase, { log: false });
  },

  submit() {
    byTestId(TEST_IDS.credentialVaultSubmit).click();
  },

  expectSubmitDisabled() {
    byTestId(TEST_IDS.credentialVaultSubmit).should("be.disabled");
  },

  expectRefusalSaying(text: string) {
    byTestId(TEST_IDS.credentialVaultError).should("be.visible").and("contain.text", text);
  },

  dismiss() {
    byTestId(TEST_IDS.credentialVaultDismiss).click();
  },

  chooseForgotPassphrase() {
    byTestId(TEST_IDS.credentialVaultForgot).click();
  },

  expectResetWarning() {
    byTestId(TEST_IDS.credentialVaultResetWarning)
      .should("be.visible")
      .and("contain.text", "set aside")
      .and("contain.text", "linked again");
  },
};
