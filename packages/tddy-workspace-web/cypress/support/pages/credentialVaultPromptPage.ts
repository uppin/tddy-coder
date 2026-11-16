import { byTestId } from "../testIds";

/** Page object for the credential vault prompt. */
export const credentialVaultPromptPage = {
  passphrase: () => byTestId("credential-vault-passphrase"),
  submit: () => byTestId("credential-vault-submit"),
  error: () => byTestId("credential-vault-error"),

  /** Type `passphrase` and submit the prompt. */
  unlockWith: (passphrase: string) => {
    credentialVaultPromptPage.passphrase().type(passphrase);
    credentialVaultPromptPage.submit().click();
  },
};
