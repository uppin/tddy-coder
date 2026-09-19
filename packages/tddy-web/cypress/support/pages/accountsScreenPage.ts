import { byTestId } from "../testIds";

const ROW_TEST_ID_PREFIX = "accounts-row-";

/**
 * Page object for the Accounts screen (`/accounts`).
 *
 * A row is addressed by `provider:accountId` rather than by `accountId` alone: an account id is
 * stable and unique only *within* its provider, and two providers naming an account `ada` is
 * ordinary rather than exceptional.
 */
export const accountsScreenPage = {
  group: (provider: string) => byTestId(`accounts-group-${provider}`),
  row: (provider: string, accountId: string) =>
    byTestId(`${ROW_TEST_ID_PREFIX}${provider}-${accountId}`),
  label: (provider: string, accountId: string) =>
    byTestId(`${ROW_TEST_ID_PREFIX}${provider}-${accountId}-label`),
  subject: (provider: string, accountId: string) =>
    byTestId(`${ROW_TEST_ID_PREFIX}${provider}-${accountId}-subject`),

  /** Shown when the vault opened and holds nothing — distinct from both states below. */
  emptyNotice: () => byTestId("accounts-empty"),
  /** Shown when this session's key no longer unwraps the vault. Re-linking is the recovery. */
  lockedNotice: () => byTestId("accounts-locked"),
  /** Shown when the read itself failed, carrying the daemon's reason verbatim. */
  errorNotice: () => byTestId("accounts-error"),

  renameField: (provider: string, accountId: string) =>
    byTestId(`${ROW_TEST_ID_PREFIX}${provider}-${accountId}-rename-input`),
  renameSubmit: (provider: string, accountId: string) =>
    byTestId(`${ROW_TEST_ID_PREFIX}${provider}-${accountId}-rename-submit`),
  removeButton: (provider: string, accountId: string) =>
    byTestId(`${ROW_TEST_ID_PREFIX}${provider}-${accountId}-remove`),
  /** Removal is destructive and irreversible from here, so it is confirmed before it is sent. */
  removeConfirm: (provider: string, accountId: string) =>
    byTestId(`${ROW_TEST_ID_PREFIX}${provider}-${accountId}-remove-confirm`),

  /** Rename an account the way a person does: type the new name, then commit it. */
  rename: (provider: string, accountId: string, label: string) => {
    accountsScreenPage.renameField(provider, accountId).clear().type(label);
    accountsScreenPage.renameSubmit(provider, accountId).click();
  },

  /** Remove an account the way a person does: ask, then confirm. */
  remove: (provider: string, accountId: string) => {
    accountsScreenPage.removeButton(provider, accountId).click();
    accountsScreenPage.removeConfirm(provider, accountId).click();
  },

  /**
   * The marker on the account this session was established with — the one `RemoveAccount` refuses
   * to forget. Absent from every other row, and absent from all of them when the session belongs
   * to no linked account.
   */
  sessionMarker: (provider: string, accountId: string) =>
    byTestId(`${ROW_TEST_ID_PREFIX}${provider}-${accountId}-session`),

  /** Begin adding another account at a provider. */
  addAccountButton: (provider: string) => byTestId(`accounts-add-${provider}`),

  /** The short code the operator types at the provider. Never the provider's own device code. */
  linkUserCode: () => byTestId("accounts-link-user-code"),
  /** Where they type it. */
  linkVerificationUri: () => byTestId("accounts-link-verification-uri"),
  /** The operator refused at the provider. Distinct from every failure below. */
  linkDeniedNotice: () => byTestId("accounts-link-denied"),
  /** The code outlived its window; a new attempt is the recovery. */
  linkExpiredNotice: () => byTestId("accounts-link-expired"),
  /** Approval succeeded and there was nowhere to put it. Nothing to do with the operator. */
  linkLockedNotice: () => byTestId("accounts-link-locked"),
};
