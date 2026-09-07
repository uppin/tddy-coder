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

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  AddHostKeyOutcome,
  ConnectionService,
  HostSshAgentSchema,
  ProbeOutcome,
  type HostSshAgent,
} from "../../src/gen/connection_pb";
import { HostAddKeyAction } from "../../src/components/hosts/HostAddKeyAction";
import { HostPassphraseDialog } from "../../src/components/hosts/HostPassphraseDialog";
import type { KeyPinVerdict } from "../../src/lib/hostKeyPinning";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aHostPromptFeed } from "../support/rpc/hostPromptFeed";
import {
  hostAddKeyOutcome,
  hostAddKeyPage as addKey,
  hostPassphraseDialogPage,
} from "../support/pages/hostsScreenPage";

const HOST = "workstation-1";
const FINGERPRINT = "SHA256:ZLBiCcwTvIcQUyRnvSHhpsdgWLLLZtWbBAPtgWNBpAg";
const PASSPHRASE = "correct horse battery staple";
const KEY_PATH = "/home/ada/.ssh/id_ed25519";

/** This spec addresses one host, so the shared dialog page object is bound to it once, here. */
const dialog = {
  root: () => hostPassphraseDialogPage.root(HOST),
  input: hostPassphraseDialogPage.input,
  submit: hostPassphraseDialogPage.submit,
  changedWarning: hostPassphraseDialogPage.changedWarning,
  unverifiedNotice: hostPassphraseDialogPage.unverifiedNotice,
};

/**
 * A real RSA-OAEP(SHA-256) public key in SPKI DER — the same shape the daemon publishes with a
 * prompt. Generated once for the suite so the encryption path is genuinely exercised: a stubbed
 * `crypto.subtle` would hollow out the one test carrying this node's security claim.
 */
let hostPublicKey: Uint8Array;

async function anRsaOaepPublicKey(): Promise<Uint8Array> {
  const keyPair = await crypto.subtle.generateKey(
    {
      name: "RSA-OAEP",
      modulusLength: 2048,
      publicExponent: new Uint8Array([0x01, 0x00, 0x01]),
      hash: "SHA-256",
    },
    true,
    ["encrypt", "decrypt"],
  );
  return new Uint8Array(await crypto.subtle.exportKey("spki", keyPair.publicKey));
}

/** What was pinned for this host before the key the dialog is showing turned up. */
const A_PINNED_FINGERPRINT = "SHA256:9WK1EJ1YHXbCP9V0Y13uwbHFuqWFcAe1eFf0kSPn5Ok";

function mountDialog(
  opts: {
    keyChanged?: boolean;
    /** What the continuity check concluded. Omitted means the test is not about the check. */
    continuity?: KeyPinVerdict;
    onSubmit?: (encrypted: Uint8Array) => void;
  } = {},
) {
  // The dialog takes the verdict and nothing else. `keyChanged` is how the tests above say "this is
  // not the key that was pinned", which is `changed` and no other arm; a test that says neither is
  // not about the check, and gets the sighting that carries no caveat.
  const keyContinuity: KeyPinVerdict =
    opts.continuity ??
    (opts.keyChanged
      ? { kind: "changed", pinnedFingerprint: A_PINNED_FINGERPRINT }
      : { kind: "unchanged" });
  mountWithRpc(
    withSelectedDaemon(
      <HostPassphraseDialog
        hostId={HOST}
        subject="id_ed25519"
        fingerprint={FINGERPRINT}
        spkiDer={hostPublicKey}
        keyContinuity={keyContinuity}
        onSubmit={opts.onSubmit ?? (() => {})}
        onCancel={() => {}}
      />,
    ),
    anInMemoryRpcBackend(),
  );
}

describe("Host add-key passphrase prompt", () => {
  before(() => {
    cy.wrap(anRsaOaepPublicKey()).then((spkiDer) => {
      hostPublicKey = spkiDer as unknown as Uint8Array;
    });
  });

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

// ---------------------------------------------------------------------------
// The continuity verdict the dialog has to say out loud
// ---------------------------------------------------------------------------

/**
 * `checkHostKey` can conclude that it concluded nothing — no key was presented, or storage refused
 * to be read. That verdict existed with no caller at all, which meant the dialog could show a
 * sighting it had verified nothing about and an operator would read it as an ordinary first use.
 */
describe("Host key continuity in the passphrase dialog", () => {
  before(() => {
    cy.wrap(anRsaOaepPublicKey()).then((spkiDer) => {
      hostPublicKey = spkiDer as unknown as Uint8Array;
    });
  });

  it("says continuity could not be checked without blocking the answer", () => {
    // Given a dialog for a host whose key could not be checked against a pin at all
    mountDialog({ continuity: { kind: "unverified" } });

    // When the operator types the passphrase
    dialog.input().type(PASSPHRASE);

    // Then the dialog says the check did not happen, and still lets the answer through: a host that
    // cannot be pin-checked is not a host that has been caught doing anything, and refusing here
    // would make the feature unusable in any browser that will not store a pin.
    dialog.unverifiedNotice().should("exist");
    // Matched as a pattern, not as copy: what has to be true is that it names the *missing check*.
    // An empty notice element would satisfy `exist` and tell an operator nothing.
    dialog.unverifiedNotice().invoke("text").should("match", /could not check/i);
    dialog.submit().should("not.be.disabled");
  });

  it("distinguishes an unverifiable host key from a changed one and from a first sighting", () => {
    // Given no conclusion could be reached
    mountDialog({ continuity: { kind: "unverified" } });
    dialog.unverifiedNotice().should("exist");
    // Not dressed up as a substitution — nobody caught this host doing anything.
    dialog.changedWarning().should("not.exist");

    // Given this host's key is not the one that was pinned
    mountDialog({ keyChanged: true, continuity: { kind: "changed", pinnedFingerprint: "SHA256:old" } });
    dialog.changedWarning().should("exist");
    dialog.unverifiedNotice().should("not.exist");

    // Given a genuine first sighting, recorded
    mountDialog({ continuity: { kind: "pinned-now" } });
    // Neither caveat: this sighting *was* checked, and it is now the pin. Letting `unverified` read
    // the same as this is exactly what leaves an active substitution looking routine.
    dialog.unverifiedNotice().should("not.exist");
    dialog.changedWarning().should("not.exist");
  });
});

// ---------------------------------------------------------------------------
// What the add came to
// ---------------------------------------------------------------------------

/** An ssh-agent that answered for this host and is holding nothing — the state that wants a key. */
function anEmptyAgent(): HostSshAgent {
  return create(HostSshAgentSchema, { outcome: ProbeOutcome.OK, reachable: true, keys: [] });
}

/**
 * A daemon that refuses the add with `outcome`.
 *
 * `AddHostKey` returns only when the add has succeeded or failed, so a handler that answers at once
 * is what a completed round trip looks like from the browser.
 */
function aBackendReporting(
  outcome: AddHostKeyOutcome,
  failureReason: string,
): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(ConnectionService, {
    ...aHostPromptFeed().handlers,
    addHostKey: async () => ({ added: false, outcome, fingerprint: "", failureReason }),
  });
}

function mountAction(backend: InMemoryRpcBackend) {
  mountWithRpc(
    withSelectedDaemon(<HostAddKeyAction instanceId={HOST} sshAgent={anEmptyAgent()} />, [
      { instanceId: HOST, label: HOST },
    ]),
    backend,
  );
}

describe("Host add-key outcomes", () => {
  it("reports a failure without adding a key when the passphrase is wrong", () => {
    // Given a host that decrypts the answer and finds it does not unlock the key
    mountAction(
      aBackendReporting(
        AddHostKeyOutcome.WRONG_PASSPHRASE,
        "the answer decrypted but did not unlock the key",
      ),
    );

    // When the operator names a key and starts the add
    addKey.addKey(HOST, KEY_PATH);

    // Then the operator is told the passphrase was the problem — the one failure here worth
    // retyping something for
    hostAddKeyOutcome.saying(HOST, /passphrase/i);

    // And nothing claims a key was loaded. A flow that surfaced the failure while leaving a success
    // standing would send an operator away believing the agent holds a key it never took.
    addKey.addedConfirmation(HOST).should("not.exist");
  });

  /**
   * The reason `AddHostKeyResponse` carries an enum rather than a bool.
   *
   * Every `failureReason` below is **empty on purpose**: with no free text to echo, the only thing
   * that can tell these three apart is the outcome. A component rendering `failureReason` verbatim
   * would pass a version of this test that supplied one, and would still be reporting "it failed"
   * three times over.
   */
  it("distinguishes a wrong passphrase from an absent agent and from an expired prompt", () => {
    mountAction(aBackendReporting(AddHostKeyOutcome.WRONG_PASSPHRASE, ""));
    addKey.addKey(HOST, KEY_PATH);
    hostAddKeyOutcome.saying(HOST, /passphrase/i);
    hostAddKeyOutcome.notSaying(HOST, /expired/i);

    // Nothing to add a key to — the operator needs an agent started, not another attempt.
    mountAction(aBackendReporting(AddHostKeyOutcome.NO_AGENT, ""));
    addKey.addKey(HOST, KEY_PATH);
    hostAddKeyOutcome.saying(HOST, /agent/i);
    hostAddKeyOutcome.notSaying(HOST, /passphrase/i);

    // The answer was fine; it arrived too late. Retyping the same passphrase is the right move, and
    // reading this as a wrong one would send the operator hunting for a passphrase that was correct.
    mountAction(aBackendReporting(AddHostKeyOutcome.PROMPT_EXPIRED, ""));
    addKey.addKey(HOST, KEY_PATH);
    hostAddKeyOutcome.saying(HOST, /expired/i);
    hostAddKeyOutcome.notSaying(HOST, /passphrase/i);
  });
});
