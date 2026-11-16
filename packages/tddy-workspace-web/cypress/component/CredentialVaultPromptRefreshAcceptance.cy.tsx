/**
 * Acceptance test: unlocking the credential vault refreshes the derived hosts (`#remote-shell` 9/11).
 *
 * The vault prompt is driven by the auth port: a locked vault asks for a passphrase, and a
 * successful `unlockVault` is the moment the hosts behind accounts become listable, so the
 * directory is asked to list them again through the derived-hosts refresh port.
 */

import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { DerivedHostsRefreshProvider } from "tddy-client-web";
import { VaultState } from "tddy-client-web/gen/auth_pb";
import { anAuthDouble } from "tddy-client-web/testkit";
import { CredentialVaultPrompt } from "../../src/signin/CredentialVaultPrompt";

const A_PASSPHRASE = "a-vault-passphrase";

describe("Unlocking the credential vault refreshes the derived hosts", () => {
  it("unlocking the vault refreshes the derived hosts", () => {
    // Given a signed-in operator whose vault is locked, under a directory that can be refreshed
    const backend = anInMemoryRpcBackend();
    const auth = anAuthDouble({ vaultState: VaultState.LOCKED });
    const refresh = cy.stub().as("refresh");
    cy.mountWithClients(
      <DerivedHostsRefreshProvider refresh={refresh}>
        <CredentialVaultPrompt />
      </DerivedHostsRefreshProvider>,
      backend,
      { auth },
    );

    // When the vault is unlocked with its passphrase
    cy.get('[data-testid="credential-vault-passphrase"]').type(A_PASSPHRASE);
    cy.get('[data-testid="credential-vault-submit"]').click();

    // Then the unlock went to the auth port, and the directory was asked to refresh, once
    cy.wrap(null).should(() => {
      expect(auth.unlockVault.mock.calls).to.deep.equal([[A_PASSPHRASE, false]]);
    });
    cy.get("@refresh").should("have.been.calledOnce");
  });

  it("does not refresh when the unlock is refused", () => {
    // Given an unlock that the daemon refuses
    const backend = anInMemoryRpcBackend();
    const auth = anAuthDouble({ vaultState: VaultState.LOCKED });
    const refusingAuth = {
      ...auth,
      unlockVault: async () => {
        throw new Error("the passphrase did not open the vault");
      },
    };
    const refresh = cy.stub().as("refresh");
    cy.mountWithClients(
      <DerivedHostsRefreshProvider refresh={refresh}>
        <CredentialVaultPrompt />
      </DerivedHostsRefreshProvider>,
      backend,
      { auth: refusingAuth },
    );

    // When
    cy.get('[data-testid="credential-vault-passphrase"]').type(A_PASSPHRASE);
    cy.get('[data-testid="credential-vault-submit"]').click();
    cy.get('[data-testid="credential-vault-error"]').should(
      "contain.text",
      "the passphrase did not open the vault",
    );

    // Then the vault never opened, so nothing changed for the directory to learn
    cy.get("@refresh").should("not.have.been.called");
  });
});
