/**
 * Acceptance tests: the passphrase prompt a host raises, and the encrypted answer that goes back.
 *
 * The load-bearing test is `sends_an_encrypted_answer_that_does_not_contain_the_passphrase`. A
 * round-trip test that only checked "the key was added" would pass just as well with the passphrase
 * in the clear — which is the entire thing this node exists to prevent. Unary calls *are* recorded
 * by the in-memory backend's interceptor, so the outgoing request body is directly assertable.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-06-agent-add-key.md
 */

import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { HostPassphraseDialog } from "../../src/components/hosts/HostPassphraseDialog";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";

const HOST = "workstation-1";
const FINGERPRINT = "SHA256:ZLBiCcwTvIcQUyRnvSHhpsdgWLLLZtWbBAPtgWNBpAg";
const PASSPHRASE = "correct horse battery staple";

const dialog = {
  root: () => cy.get(`[data-testid="host-passphrase-dialog-${HOST}"]`),
  input: () => cy.get('[data-testid="host-passphrase-input"]'),
  submit: () => cy.get('[data-testid="host-passphrase-submit"]'),
  changedWarning: () => cy.get('[data-testid="host-key-changed-warning"]'),
};

function mountDialog(opts: { keyChanged?: boolean; onSubmit?: (p: string) => void } = {}) {
  mountWithRpc(
    withSelectedDaemon(
      <HostPassphraseDialog
        hostId={HOST}
        subject="id_ed25519"
        fingerprint={FINGERPRINT}
        keyChanged={opts.keyChanged ?? false}
        onSubmit={opts.onSubmit ?? (() => {})}
        onCancel={() => {}}
      />,
    ),
    anInMemoryRpcBackend(),
  );
}

describe("Host add-key passphrase prompt", () => {
  it("surfaces a passphrase prompt naming the host and the key", () => {
    mountDialog();

    dialog.root().should("contain.text", HOST);
    dialog.root().should("contain.text", "id_ed25519");
  });

  it("shows the hosts public key fingerprint in the dialog", () => {
    mountDialog();

    // Shown in full so an operator can compare it against the host out of band.
    dialog.root().should("contain.text", FINGERPRINT);
  });

  it("blocks the flow with a warning when a hosts key has changed", () => {
    mountDialog({ keyChanged: true });

    dialog.changedWarning().should("exist");
    // Blocked, not merely warned: a changed key is exactly the substitution the pin exists to catch.
    dialog.submit().should("be.disabled");
  });

  it("sends an encrypted answer that does not contain the passphrase", () => {
    const submitted: Uint8Array[] = [];
    mountDialog({
      onSubmit: (encrypted: unknown) => submitted.push(encrypted as Uint8Array),
    });

    dialog.input().type(PASSPHRASE);
    dialog.submit().click();

    cy.then(() => {
      expect(submitted, "the dialog submits exactly one answer").to.have.length(1);
      const asText = new TextDecoder().decode(submitted[0]);
      expect(
        asText,
        "the passphrase must never leave the browser in the clear",
      ).to.not.contain(PASSPHRASE);
      expect(submitted[0].byteLength, "an RSA-OAEP ciphertext is key-sized").to.be.greaterThan(64);
    });
  });

  it("does not echo the passphrase back into the dom after submission", () => {
    mountDialog();

    dialog.input().type(PASSPHRASE);
    dialog.submit().click();

    // A cleared field is the difference between a secret held for a moment and one left on screen.
    dialog.input().should("have.value", "");
    dialog.root().should("not.contain.text", PASSPHRASE);
  });
});
