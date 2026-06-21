import { byTestId, TEST_IDS } from "../testIds";

export const codexOAuthDialogPage = {
  dialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.codexOauthDialog, options),
  dismiss: () => byTestId(TEST_IDS.codexOauthDismiss),
  embeddingFallback: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.codexOauthEmbeddingFallback, options),
};
