/**
 * Acceptance tests: the passphrase prompt a host raises, and the encrypted answer that goes back.
 *
 * The load-bearing test is `sends_an_encrypted_answer_that_does_not_contain_the_passphrase`. A
 * round-trip test that only checked "the key was added" would pass just as well with the passphrase
 * in the clear — which is the entire thing this node exists to prevent. Unary calls *are* recorded
 * by the in-memory backend's interceptor, so the outgoing request body is directly assertable.
 *
 * Feature: docs/ft/web/hosts-screen-add-key.md
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  AddHostKeyOutcome,
  HostService,
  HostSshAgentSchema,
  ProbeOutcome,
  type HostSshAgent,
} from "../../src/gen/host_pb";
import { HostAddKeyAction } from "../../src/components/hosts/HostAddKeyAction";
import { HostPassphraseDialog } from "../../src/components/hosts/HostPassphraseDialog";
import type { KeyPinVerdict } from "../../src/lib/hostKeyPinning";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aHostPromptFeed, type HostPromptFeed } from "../support/rpc/hostPromptFeed";
import {
  anRsaOaepKeypair,
  anRsaOaepPublicKey,
  fingerprintOf,
  hostKeyPins,
  type AHostPromptKeypair,
} from "../support/hostKeys";
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
  underivableNotice: hostPassphraseDialogPage.underivableNotice,
};

/**
 * A real RSA-OAEP(SHA-256) public key in SPKI DER — the same shape the daemon publishes with a
 * prompt. Generated once per suite so the encryption path is genuinely exercised: a stubbed
 * `crypto.subtle` would hollow out the one test carrying this node's security claim.
 */
let hostPublicKey: Uint8Array;
/** The fingerprint that host honestly advertises — a digest of the key above, never a bare string. */
let hostKeyFingerprint: string;
/**
 * The same key with the private half kept, for the one spec that reads the answer back.
 *
 * Held apart from `hostPublicKey` because only the wire-level test needs it: a dialog that hands
 * its own `onSubmit` a ciphertext is a claim about the dialog, and the claim about the *answer* is
 * that this host — and only this host — can decrypt it.
 */
let hostKeypair: AHostPromptKeypair;

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
        onAcceptChangedKey={() => {}}
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
      // Exactly one block under a 2048-bit key. `> 64` also held for a digest, or for a base64
      // re-encoding of the passphrase itself.
      expect(submitted[0].byteLength, "a 2048-bit RSA-OAEP block").to.equal(256);
    });
  });

  it("refuses to send an empty passphrase, which unlocks no key", () => {
    // Given a prompt for an encrypted key, with nothing typed
    const submitted: Uint8Array[] = [];
    mountDialog({ onSubmit: (encrypted: unknown) => submitted.push(encrypted as Uint8Array) });

    // When the operator tries to send it anyway
    dialog.submit().click({ force: true });

    // Then nothing went back to the host. Asserted on what left rather than on the control, so a
    // dialog that quietly accepted an empty answer would fail here even with the button still
    // rendered disabled — an empty passphrase unlocks no encrypted key, and answering with one
    // spends the single-use prompt the host is blocked on.
    cy.then(() => {
      expect(submitted, "an empty passphrase is not an answer").to.have.length(0);
    });

    // …and the dialog says so, rather than swallowing the click without explanation.
    dialog.submit().should("be.disabled");
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

  it("blocks the answer and names the origin when the browser cannot encrypt at all", () => {
    // Given a page served over plain http, where `crypto.subtle` does not exist — the origin the
    // daemon actually serves this bundle on, and one no browser test can produce, since Cypress
    // runs on localhost and localhost is a secure context
    mountDialog({
      continuity: {
        kind: "underivable",
        reason:
          "this browser exposes no Web Crypto (crypto.subtle) on this page — browsers offer it " +
          "only in a secure context",
      },
    });

    // When the operator looks at the dialog
    // Then nothing can be typed or sent, and the reason is the origin rather than the host. A
    // plaintext fallback here would hand the passphrase to every peer in the common room.
    dialog.underivableNotice().invoke("text").should("match", /secure context/i);
    dialog.input().should("be.disabled");
    dialog.submit().should("be.disabled");
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
  return anInMemoryRpcBackend().implement(HostService, {
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

// ---------------------------------------------------------------------------
// What actually goes on the wire
// ---------------------------------------------------------------------------

/**
 * The load-bearing assertion of this node, made against the **request body** rather than against
 * what the dialog handed its own `onSubmit` prop.
 *
 * The dialog-level test above proves the dialog; this proves the wire. They are not the same claim:
 * a caller that took the ciphertext and sent the passphrase beside it would pass the first and fail
 * this one, and `AnswerHostPrompt` is unary, so the in-memory backend records exactly what left.
 */
function aBackendAwaitingAnAnswer(feed: HostPromptFeed): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(HostService, {
    ...feed.handlers,
    // Never settles: the host is blocked on the passphrase, which is why the prompt exists.
    addHostKey: () => new Promise(() => undefined),
    answerHostPrompt: async () => ({ accepted: true, rejectionReason: "" }),
  });
}

describe("The answer AnswerHostPrompt carries", () => {
  before(() => {
    cy.wrap(anRsaOaepKeypair())
      .then((generated) => {
        hostKeypair = generated as unknown as AHostPromptKeypair;
        hostPublicKey = hostKeypair.spkiDer;
        return fingerprintOf(hostPublicKey);
      })
      .then((fingerprint) => {
        hostKeyFingerprint = fingerprint as unknown as string;
      });
  });

  beforeEach(() => {
    cy.clearLocalStorage();
  });

  it("carries a ciphertext and no passphrase in the request the browser sends", () => {
    // Given a host blocked on a passphrase for a key the operator named
    const feed = aHostPromptFeed();
    const backend = aBackendAwaitingAnAnswer(feed);
    mountAction(backend);
    addKey.addKey(HOST, KEY_PATH);
    cy.wrap(feed).should((f: HostPromptFeed) => expect(f.subscriptionCount()).to.equal(1));
    cy.then(() => {
      feed.raise({
        promptId: "prompt-1",
        daemonInstanceId: HOST,
        subject: KEY_PATH,
        hostPublicKey,
        hostPublicKeyFingerprint: hostKeyFingerprint,
      });
    });

    // When the operator answers it
    dialog.input().type(PASSPHRASE);
    dialog.submit().click();

    // Then the passphrase is nowhere in the payload that left, and what did leave is an RSA-OAEP
    // block under this host's published 2048-bit key — the entire reason this node exists
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      const calls = b.callsTo(HostService.method.answerHostPrompt);
      expect(calls, "exactly one answer is sent for one prompt").to.have.length(1);
      expect(calls[0].promptId).to.equal("prompt-1");
      expect(
        new TextDecoder().decode(calls[0].encryptedAnswer),
        "the passphrase must never leave the browser in the clear",
      ).to.not.contain(PASSPHRASE);
      expect(calls[0].encryptedAnswer.byteLength, "a 2048-bit RSA-OAEP block").to.equal(256);
    });

    // And the host can read it back: decrypted with the private half of the key the prompt
    // published, the answer is exactly the passphrase. Everything above holds for a digest, or for
    // 256 random bytes — neither of which is an answer this host could ever unlock a key with.
    cy.then(() => {
      const [answer] = backend.callsTo(HostService.method.answerHostPrompt);
      return hostKeypair.decrypt(answer.encryptedAnswer);
    }).should("equal", PASSPHRASE);
  });
});

// ---------------------------------------------------------------------------
// The key the dialog is holding, and the key it is describing
// ---------------------------------------------------------------------------

/**
 * A hold on fingerprint derivation, so a spec can stand *inside* the window between a key arriving
 * and its digest coming back.
 *
 * That window is where the correlation between "the key that encrypts" and "the fingerprint that is
 * displayed and pinned" can come apart, and it is not otherwise observable: derivation resolves in a
 * microtask, so by the time any retried assertion runs, the state has settled. A peer in the routing
 * path does not have that problem — it can raise frames faster than SHA-256 resolves, for as long as
 * it likes — so the hold is how a test occupies the position an attacker occupies for free.
 *
 * Patched onto `crypto.subtle` itself rather than onto the module that calls it, because the claim
 * being tested is about what is on screen while a *real* derivation is outstanding.
 */
interface ADigestHold {
  /** Let every derivation held so far complete. */
  readonly release: () => void;
  /** How many derivations are waiting. */
  readonly held: () => number;
  /** Put the browser's own `digest` back. */
  readonly restore: () => void;
}

function holdDerivationsAfter(letThrough: number): ADigestHold {
  const subtle = crypto.subtle;
  const real = subtle.digest.bind(subtle);
  const waiting: Array<() => void> = [];
  let seen = 0;
  const patched = (algorithm: AlgorithmIdentifier, data: BufferSource): Promise<ArrayBuffer> => {
    seen += 1;
    if (seen <= letThrough) return real(algorithm, data);
    return new Promise<void>((resolve) => waiting.push(resolve)).then(() =>
      real(algorithm, data),
    );
  };
  Object.defineProperty(subtle, "digest", {
    value: patched,
    configurable: true,
    writable: true,
  });
  return {
    release: () => {
      while (waiting.length > 0) (waiting.shift() as () => void)();
    },
    held: () => waiting.length,
    restore: () => {
      Reflect.deleteProperty(subtle, "digest");
    },
  };
}

/**
 * The key a prompt carries and the verdict shown beside it have to be **the same key's**.
 *
 * `HostAddKeyAction` hands the dialog `spkiDer` from the prompt and `fingerprint`/`keyContinuity`
 * from a separate piece of state, with nothing tying the two together. A second prompt frame
 * replaces the key immediately; the verdict for it arrives a digest later. In between, the dialog
 * shows the *previous* key's fingerprint and its reassuring `unchanged` verdict while holding bytes
 * that will encrypt the passphrase for a different key — which is precisely the substitution the pin
 * exists to catch, wearing the pin's own approval.
 *
 * Nothing else in this suite raises a second prompt while a dialog is open.
 */
describe("A second prompt arriving while the dialog is open", () => {
  /** The key the operator has already trusted for this host, and the one that replaces it. */
  let trustedKey: Uint8Array;
  let trustedFingerprint: string;
  let substitutedKey: Uint8Array;
  let substitutedFingerprint: string;
  let hold: ADigestHold | null = null;

  before(() => {
    cy.wrap(anRsaOaepPublicKey())
      .then((spkiDer) => {
        trustedKey = spkiDer as unknown as Uint8Array;
        return fingerprintOf(trustedKey);
      })
      .then((fingerprint) => {
        trustedFingerprint = fingerprint as unknown as string;
        return anRsaOaepPublicKey();
      })
      .then((spkiDer) => {
        substitutedKey = spkiDer as unknown as Uint8Array;
        return fingerprintOf(substitutedKey);
      })
      .then((fingerprint) => {
        substitutedFingerprint = fingerprint as unknown as string;
      });
  });

  beforeEach(() => {
    cy.clearLocalStorage();
  });

  afterEach(() => {
    hold?.restore();
    hold = null;
  });

  it("never shows the previous keys fingerprint beside the key that would encrypt", () => {
    // Given a host whose key this browser has already pinned, and an add blocked on its prompt
    const feed = aHostPromptFeed();
    const backend = aBackendAwaitingAnAnswer(feed);
    mountAction(backend);
    cy.then(() => hostKeyPins.pin(HOST, trustedFingerprint));
    addKey.addKey(HOST, KEY_PATH);
    cy.wrap(feed).should((f: HostPromptFeed) => expect(f.subscriptionCount()).to.equal(1));
    cy.then(() => {
      feed.raise({
        promptId: "prompt-1",
        daemonInstanceId: HOST,
        subject: KEY_PATH,
        hostPublicKey: trustedKey,
        hostPublicKeyFingerprint: trustedFingerprint,
      });
    });
    // The pinned key, checked and unremarkable — the state an operator would answer without a
    // second thought, and therefore the state worth substituting under.
    dialog.root().should("contain.text", trustedFingerprint);
    dialog.changedWarning().should("not.exist");

    // When a second frame replaces the key while the dialog is open, and its digest is not back yet
    cy.then(() => {
      hold = holdDerivationsAfter(0);
      feed.raise({
        promptId: "prompt-2",
        daemonInstanceId: HOST,
        subject: KEY_PATH,
        hostPublicKey: substitutedKey,
        hostPublicKeyFingerprint: substitutedFingerprint,
      });
    });

    // Then the dialog says nothing about the key it used to be holding. Showing the old
    // fingerprint here is showing an operator a value they may have verified out of band, bound to
    // bytes that are not the ones it describes.
    dialog.root().should("not.contain.text", trustedFingerprint);
    // And no answer can be sent while the key in hand is unaccounted for — a submit that is merely
    // absent satisfies this too, which is why it is stated as "nothing enabled" rather than as the
    // presence of any particular control.
    cy.get('[data-testid="host-passphrase-submit"]:not([disabled])').should("not.exist");

    // And once the check for the new key does come back, it is the substitution it actually is
    cy.then(() => hold?.release());
    dialog.changedWarning().should("exist");
    dialog.submit().should("be.disabled");
  });
});

// ---------------------------------------------------------------------------
// The path the field invites an operator to type
// ---------------------------------------------------------------------------

/**
 * **The UI must not invite a path the host is bound to refuse.**
 *
 * `confined_to_home` (`packages/tddy-daemon/src/host_private_key.rs`) requires an absolute path,
 * and nothing anywhere expands `~` — not the browser, which does not know the host's home, and not
 * the daemon, whose confinement is deliberately lexical and touches the filesystem not at all. So
 * the example this field has been showing since it shipped, `~/.ssh/id_ed25519`, is refused with
 * `KEY_OUTSIDE_HOME`.
 *
 * **The resolution pinned here is absolute paths only**, in the field and in the picker alike:
 *
 * - `ListHostKeyCandidates` returns absolute paths, so the common case is picking, not typing, and
 *   one path syntax across both surfaces is the only way a picked path and a typed one can be
 *   compared by eye.
 * - Expanding `~` would have to happen in the daemon, and the confinement's whole value is that it
 *   is a function of the caller's own input — no `canonicalize`, no `stat`, nothing that could
 *   answer "does this exist?". A second path syntax to reason about is a poor trade for saving five
 *   characters.
 * - The refusal cannot explain itself. `KEY_OUTSIDE_HOME` names no path, on purpose, so an operator
 *   who types the placeholder learns only that their key "must be a path inside your own home" —
 *   about a path that *was* inside their home. The browser holds the one piece of context that
 *   makes that refusal legible, so it is the browser that must not send the request.
 */
describe("The key path the add-key field invites", () => {
  it("shows an example the host will accept rather than a tilde path it refuses", () => {
    // Given the add-key control on a host with an agent
    mountAction(aBackendReporting(AddHostKeyOutcome.KEY_UNREADABLE, ""));

    // When the operator reads the example in the field
    // Then it is an absolute path — the only kind `confined_to_home` accepts
    addKey.keyField(HOST).invoke("attr", "placeholder").should("match", /^\//);
    addKey.keyField(HOST).invoke("attr", "placeholder").should("not.contain", "~");
  });

  it("does not send a tilde path for the host to refuse without explanation", () => {
    // Given a host that would report a key it cannot read, saying nothing about why
    const backend = aBackendReporting(AddHostKeyOutcome.KEY_UNREADABLE, "");
    mountAction(backend);

    // When the operator types a path relative to their home and asks for the add
    addKey.addKey(HOST, "~/.ssh/id_ed25519");

    // Then nothing was sent, and the operator is told what is wrong with the path they typed —
    // which is knowledge only this side has: the daemon's refusal is deliberately silent about it
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      expect(
        b.callsTo(HostService.method.addHostKey),
        "a path the host cannot accept was sent anyway",
      ).to.have.length(0);
    });
    hostAddKeyOutcome.saying(HOST, /absolute/i);
  });
});
