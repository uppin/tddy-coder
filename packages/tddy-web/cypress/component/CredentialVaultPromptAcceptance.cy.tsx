/**
 * Acceptance tests: the credential vault prompt.
 *
 * The daemon keeps each operator's GitHub token in a vault sealed under a key derived from a
 * passphrase only they know. Signing in always completes; the session then says where the vault
 * stands, and this prompt is how the operator acts on it:
 *
 * - **LOCKED** — a vault exists and is closed on the daemon (it restarted, or this is a fresh
 *   browser). The prompt asks for the passphrase. A forgotten one can be reset, after a warning:
 *   the old vault is set aside, not deleted, and its credentials must be linked again.
 * - **UNINITIALIZED** — no vault yet. The prompt asks the operator to choose a passphrase, typed
 *   twice.
 * - **OPEN** / **NONE** — nothing to do, and no prompt.
 *
 * Opening the vault hands this browser an unlock key, kept beside the refresh token, so a later
 * restart of the daemon is recovered by the next session refresh without asking again.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-09-19-keyring-store.md
 * Changeset: docs/dev/1-WIP/2026-09-19-keyring-store.md
 */

import React from "react";
import { ConnectError, Code } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AuthProvider, useAuthContext } from "../../src/hooks/authProvider";
import { AuthService, VaultState } from "../../src/gen/auth_pb";
import { CredentialVaultPrompt } from "../../src/components/CredentialVaultPrompt";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aGitHubUser } from "../support/rpc/responses";
import {
  ACCESS_TOKEN_KEY,
  CURRENT_ACCESS_TOKEN,
  REFRESH_TOKEN_KEY,
  VALID_REFRESH_TOKEN,
} from "../support/rpc/durableSessionBackend";
import { credentialVaultPromptPage } from "../support/pages/credentialVaultPromptPage";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THE_PASSPHRASE = "correct horse battery staple";
const A_WRONG_PASSPHRASE = "incorrect horse battery staple";
const A_NEW_PASSPHRASE = "a passphrase chosen after forgetting";
const THE_UNLOCK_KEY = "6f70657261746f72.slot.unlocked";
const VAULT_UNLOCK_KEY_KEY = "tddy_vault_unlock_key";

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/**
 * A daemon on which the signed-in operator's vault stands at `vaultState`, and opens under
 * `THE_PASSPHRASE` only — as the real one does, a wrong passphrase is `FailedPrecondition`.
 */
function aDaemonWhoseVaultIs(vaultState: VaultState): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(AuthService, {
    getAuthStatus: async () => ({ authenticated: true, user: aGitHubUser(), vaultState }),
    unlockVault: async (req: { passphrase?: string }) => {
      if (vaultState === VaultState.LOCKED && req.passphrase !== THE_PASSPHRASE) {
        throw new ConnectError("the credential vault is locked: that key does not open it", Code.FailedPrecondition);
      }
      return { vaultState: VaultState.OPEN, vaultUnlockKey: THE_UNLOCK_KEY };
    },
    resetVault: async () => ({ vaultState: VaultState.OPEN, vaultUnlockKey: THE_UNLOCK_KEY }),
  });
}

/** Renders the shared auth context's view of the vault beside the prompt under test. */
function VaultStateProbe() {
  const { isAuthenticated, vaultState } = useAuthContext();
  return <div data-testid={TEST_IDS.authProbeStatus}>{isAuthenticated ? `vault:${VaultState[vaultState]}` : "signed-out"}</div>;
}

/** Wait until the page has learned where the vault stands, so an absent prompt means something. */
function expectThePageToKnowTheVaultIs(vaultState: VaultState) {
  byTestId(TEST_IDS.authProbeStatus).should("have.text", `vault:${VaultState[vaultState]}`);
}

/** A signed-in operator — a stored session, no unlock key yet — looking at the prompt. */
function givenASignedInOperatorOn(backend: InMemoryRpcBackend) {
  cy.clearLocalStorage();
  cy.window().then((win) => {
    win.localStorage.setItem(ACCESS_TOKEN_KEY, CURRENT_ACCESS_TOKEN);
    win.localStorage.setItem(REFRESH_TOKEN_KEY, VALID_REFRESH_TOKEN);
  });
  mountWithRpc(
    <AuthProvider>
      <CredentialVaultPrompt />
      <VaultStateProbe />
    </AuthProvider>,
    backend,
  );
}

function expectUnlockRequests(backend: InMemoryRpcBackend, requests: Array<[string, boolean]>) {
  cy.wrap(null).should(() => {
    expect(
      backend
        .callsTo(AuthService.method.unlockVault)
        .map((request) => [request.passphrase, request.create] as [string, boolean]),
      "UnlockVault requests",
    ).to.deep.equal(requests);
  });
}

function expectResetRequests(backend: InMemoryRpcBackend, newPassphrases: string[]) {
  cy.wrap(null).should(() => {
    expect(
      backend.callsTo(AuthService.method.resetVault).map((request) => request.newPassphrase),
      "ResetVault requests",
    ).to.deep.equal(newPassphrases);
  });
}

function expectStoredUnlockKey(key: string | null) {
  cy.window().should((win) => {
    expect(win.localStorage.getItem(VAULT_UNLOCK_KEY_KEY), "stored vault unlock key").to.equal(key);
  });
}

// ---------------------------------------------------------------------------
// A locked vault
// ---------------------------------------------------------------------------

describe("Credential vault prompt — a locked vault", () => {
  it("asks for the passphrase", () => {
    // Given / When
    givenASignedInOperatorOn(aDaemonWhoseVaultIs(VaultState.LOCKED));

    // Then
    credentialVaultPromptPage.expectAskingToUnlock();
  });

  it("unlocks with the passphrase, keeps the unlock key it is handed, and goes away", () => {
    // Given
    const backend = aDaemonWhoseVaultIs(VaultState.LOCKED);
    givenASignedInOperatorOn(backend);

    // When
    credentialVaultPromptPage.typePassphrase(THE_PASSPHRASE);
    credentialVaultPromptPage.submit();

    // Then
    expectUnlockRequests(backend, [[THE_PASSPHRASE, false]]);
    expectStoredUnlockKey(THE_UNLOCK_KEY);
    expectThePageToKnowTheVaultIs(VaultState.OPEN);
    credentialVaultPromptPage.expectNoPrompt();
  });

  it("says the vault is still locked when the passphrase is wrong, and stays", () => {
    // Given
    givenASignedInOperatorOn(aDaemonWhoseVaultIs(VaultState.LOCKED));

    // When
    credentialVaultPromptPage.typePassphrase(A_WRONG_PASSPHRASE);
    credentialVaultPromptPage.submit();

    // Then
    credentialVaultPromptPage.expectRefusalSaying("locked");
    credentialVaultPromptPage.expectAskingToUnlock();
    expectStoredUnlockKey(null);
  });

  it("warns before a reset that the old vault is set aside and its credentials must be linked again", () => {
    // Given
    givenASignedInOperatorOn(aDaemonWhoseVaultIs(VaultState.LOCKED));

    // When
    credentialVaultPromptPage.chooseForgotPassphrase();

    // Then
    credentialVaultPromptPage.expectResetWarning();
  });

  it("resets under a new passphrase typed twice, and goes away", () => {
    // Given
    const backend = aDaemonWhoseVaultIs(VaultState.LOCKED);
    givenASignedInOperatorOn(backend);
    credentialVaultPromptPage.chooseForgotPassphrase();

    // When
    credentialVaultPromptPage.typePassphrase(A_NEW_PASSPHRASE);
    credentialVaultPromptPage.typeConfirmation(A_NEW_PASSPHRASE);
    credentialVaultPromptPage.submit();

    // Then
    expectResetRequests(backend, [A_NEW_PASSPHRASE]);
    expectStoredUnlockKey(THE_UNLOCK_KEY);
    expectThePageToKnowTheVaultIs(VaultState.OPEN);
    credentialVaultPromptPage.expectNoPrompt();
  });
});

// ---------------------------------------------------------------------------
// A vault not created yet
// ---------------------------------------------------------------------------

describe("Credential vault prompt — no vault yet", () => {
  it("asks the operator to choose a passphrase and confirm it", () => {
    // Given / When
    givenASignedInOperatorOn(aDaemonWhoseVaultIs(VaultState.UNINITIALIZED));

    // Then
    credentialVaultPromptPage.expectAskingToCreate();
  });

  it("will not create the vault until both fields match", () => {
    // Given
    givenASignedInOperatorOn(aDaemonWhoseVaultIs(VaultState.UNINITIALIZED));

    // When
    credentialVaultPromptPage.typePassphrase(THE_PASSPHRASE);
    credentialVaultPromptPage.typeConfirmation(A_WRONG_PASSPHRASE);

    // Then
    credentialVaultPromptPage.expectSubmitDisabled();
  });

  it("creates the vault under the chosen passphrase, and goes away", () => {
    // Given
    const backend = aDaemonWhoseVaultIs(VaultState.UNINITIALIZED);
    givenASignedInOperatorOn(backend);

    // When
    credentialVaultPromptPage.typePassphrase(THE_PASSPHRASE);
    credentialVaultPromptPage.typeConfirmation(THE_PASSPHRASE);
    credentialVaultPromptPage.submit();

    // Then
    expectUnlockRequests(backend, [[THE_PASSPHRASE, true]]);
    expectThePageToKnowTheVaultIs(VaultState.OPEN);
    credentialVaultPromptPage.expectNoPrompt();
  });
});

// ---------------------------------------------------------------------------
// Nothing to do
// ---------------------------------------------------------------------------

describe("Credential vault prompt — nothing to unlock", () => {
  it("does not appear while the vault is open", () => {
    // Given / When
    givenASignedInOperatorOn(aDaemonWhoseVaultIs(VaultState.OPEN));

    // Then
    expectThePageToKnowTheVaultIs(VaultState.OPEN);
    credentialVaultPromptPage.expectNoPrompt();
  });

  it("does not appear for a login the daemon keeps no vault for", () => {
    // Given / When
    givenASignedInOperatorOn(aDaemonWhoseVaultIs(VaultState.NONE));

    // Then
    expectThePageToKnowTheVaultIs(VaultState.NONE);
    credentialVaultPromptPage.expectNoPrompt();
  });
});
