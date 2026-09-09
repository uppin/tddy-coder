/**
 * Cypress component acceptance: the add-key action on a host row.
 *
 * This is the surface that *starts* the whole node. The daemon can serve the round trip — raise a
 * prompt, take an encrypted answer, unlock a key, hand it to the agent — but until something in the
 * browser issues `AddHostKey` none of it is reachable by an operator.
 *
 * The action is mounted on its own rather than through `HostsScreen`, the way
 * `HostsScreenSshAgentAcceptance` mounts `HostRowSshAgent`: a failure in the action's own behaviour
 * should name this node, not the one that assembles rows.
 *
 * **What this file deliberately does not test.** Letting the prompt subscription go is invisible
 * here — `createRouterTransport` propagates neither an abort nor a consumer's `break` to the server
 * handler, so no counter on this fake can fall back when a stream closes. That property is pinned by
 * `src/rpc/hostPromptsSubscription.test.ts` instead.
 *
 * Feature: docs/ft/web/hosts-screen-add-key.md
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  AddHostKeyOutcome,
  ConnectionService,
  HostSshAgentSchema,
  ProbeOutcome,
  SshAgentKeySchema,
  type HostSshAgent,
} from "../../src/gen/connection_pb";
import { HostAddKeyAction } from "../../src/components/hosts/HostAddKeyAction";
import { HostRowSshAgent } from "../../src/components/hosts/HostRowSshAgent";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aHostPromptFeed, type HostPromptFeed } from "../support/rpc/hostPromptFeed";
import {
  hostAddKeyOutcome,
  hostAddKeyPage as addKey,
  hostPassphraseDialogPage as dialog,
} from "../support/pages/hostsScreenPage";
import { anRsaOaepPublicKey, fingerprintOf, hostKeyPins } from "../support/hostKeys";

const HOST = "workstation-1";
const KEY_PATH = "/home/ada/.ssh/id_ed25519";
const ADDED_FINGERPRINT = "SHA256:JfISx02kSjWJevGy/MjUdXCv76HaRM3YkYNvepTyHD8";
const PASSPHRASE = "correct horse battery staple";
/** A key this browser once pinned for the host, before the key it is being shown now. */
const A_PREVIOUSLY_PINNED_KEY = "SHA256:9WK1EJ1YHXbCP9V0Y13uwbHFuqWFcAe1eFf0kSPn5Ok";

/**
 * Real key material for the host, and for a peer standing in front of it.
 *
 * Not a stand-in byte string: the browser now fingerprints the key a prompt carries and refuses a
 * prompt that advertises a fingerprint of anything else, so a fake key needs a real digest — and a
 * substitution needs a second key that is genuinely a different one.
 */
let hostPublicKey: Uint8Array;
let hostKeyFingerprint: string;
let aPeersPublicKey: Uint8Array;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** An ssh-agent that answered for this host and is holding nothing — the state that wants a key. */
function anEmptyAgent(): HostSshAgent {
  return create(HostSshAgentSchema, { outcome: ProbeOutcome.OK, reachable: true, keys: [] });
}

/** An ssh-agent that answered and is already holding a key. */
function anAgentHolding(fingerprint: string): HostSshAgent {
  return create(HostSshAgentSchema, {
    outcome: ProbeOutcome.OK,
    reachable: true,
    keys: [create(SshAgentKeySchema, { keyType: "ssh-ed25519", fingerprint, comment: "ada@ws" })],
  });
}

/** No agent answered for this host's OS user — there is nothing to add a key to. */
function noAgent(): HostSshAgent {
  return create(HostSshAgentSchema, { outcome: ProbeOutcome.OK, reachable: false, keys: [] });
}

/**
 * A daemon that accepts the add and reports the fingerprint its agent now holds.
 *
 * `AddHostKey` returns only when the add has succeeded or failed, so this handler standing in for a
 * completed round trip is what the real call looks like from the browser.
 */
function aBackendThatAdds(feed: HostPromptFeed): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(ConnectionService, {
    ...feed.handlers,
    addHostKey: async () => ({
      added: true,
      outcome: AddHostKeyOutcome.ADDED,
      fingerprint: ADDED_FINGERPRINT,
      failureReason: "",
    }),
  });
}

/**
 * A daemon that raises a prompt and then stays blocked on the answer — the state an operator is
 * actually looking at while the dialog is up.
 */
function aBackendAwaitingAnAnswer(feed: HostPromptFeed): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(ConnectionService, {
    ...feed.handlers,
    // Never settles: the host is waiting on the passphrase, which is the whole point of the prompt.
    addHostKey: () => new Promise(() => undefined),
  });
}

function mountAction(sshAgent: HostSshAgent, backend: InMemoryRpcBackend) {
  mountWithRpc(
    withSelectedDaemon(<HostAddKeyAction instanceId={HOST} sshAgent={sshAgent} />, [
      { instanceId: HOST, label: HOST },
    ]),
    backend,
  );
}

/**
 * The ssh-agent cell of a host row, as `HostsScreen` assembles it — the action *and* what renders
 * it, rather than the action alone.
 *
 * The one thing mounting the action directly cannot say is whether an operator can reach it. See
 * the test that uses this.
 */
function mountRow(sshAgent: HostSshAgent, backend: InMemoryRpcBackend) {
  mountWithRpc(
    withSelectedDaemon(<HostRowSshAgent instanceId={HOST} sshAgent={sshAgent} />, [
      { instanceId: HOST, label: HOST },
    ]),
    backend,
  );
}

/**
 * The operator names a key and starts the add; the host then raises its passphrase question.
 *
 * `advertisedFingerprint` is stated separately from `key` because the two are separate fields on the
 * wire, and a peer that substitutes one can leave the other alone — which is the whole subject of
 * the tests below.
 */
function theHostAsksForAPassphrase(
  feed: HostPromptFeed,
  key: Uint8Array,
  advertisedFingerprint: string,
) {
  addKey.addKey(HOST, KEY_PATH);
  cy.wrap(feed).should((f: HostPromptFeed) => expect(f.subscriptionCount()).to.equal(1));
  cy.then(() => {
    feed.raise({
      promptId: "prompt-1",
      daemonInstanceId: HOST,
      subject: KEY_PATH,
      hostPublicKey: key,
      hostPublicKeyFingerprint: advertisedFingerprint,
    });
  });
}

// ---------------------------------------------------------------------------

describe("Hosts screen add key", () => {
  before(() => {
    cy.wrap(anRsaOaepPublicKey())
      .then((spkiDer) => {
        hostPublicKey = spkiDer as unknown as Uint8Array;
        return fingerprintOf(hostPublicKey);
      })
      .then((fingerprint) => {
        hostKeyFingerprint = fingerprint as unknown as string;
        return anRsaOaepPublicKey();
      })
      .then((spkiDer) => {
        aPeersPublicKey = spkiDer as unknown as Uint8Array;
      });
  });

  beforeEach(() => {
    cy.clearLocalStorage();
  });

  it("offers to add a key when an ssh-agent is reachable", () => {
    // Given a host whose agent answered and is holding nothing
    const backend = aBackendThatAdds(aHostPromptFeed());

    // When its row renders
    mountAction(anEmptyAgent(), backend);

    // Then the operator is offered somewhere to name a key and something to press
    addKey.action(HOST).should("exist");
    addKey.keyField(HOST).should("be.enabled");
    addKey.start(HOST).should("exist");
  });

  it("offers to add a key to an agent that is already holding one", () => {
    // Given an agent already holding a key — a host commonly needs a second
    const backend = aBackendThatAdds(aHostPromptFeed());

    // When its row renders
    mountAction(anAgentHolding(ADDED_FINGERPRINT), backend);

    // Then the add is still offered: "holding a key" is not "holding every key this host needs"
    addKey.action(HOST).should("exist");
  });

  /**
   * Stated as a contrast rather than as a bare absence.
   *
   * "No control on an unreachable host" is satisfied by a row that renders no control on *any*
   * host, which is exactly the state this node starts from — so on its own that assertion pins
   * nothing. Mounting both states in one test is the same shape
   * `HostsScreenSshAgentAcceptance`'s "distinguishes an empty agent from an absent one" uses, and
   * for the same reason.
   */
  it("offers the add only where there is an agent to add to", () => {
    // Given a host whose agent answered, the add is offered
    mountAction(anEmptyAgent(), aBackendThatAdds(aHostPromptFeed()));
    addKey.action(HOST).should("exist");

    // Given a host whose agent did not answer, there is no add-key control at all. That host needs
    // an agent started; a control that could only ever fail is worse than none.
    mountAction(noAgent(), aBackendThatAdds(aHostPromptFeed()));
    addKey.action(HOST).should("not.exist");
  });

  /**
   * The add-key control has to be reachable from the row, and nothing else here says so.
   *
   * Every other test in this file mounts `HostAddKeyAction` on its own, so the line in
   * `HostRowSshAgent` that renders it is unpinned: delete it and the feature is unreachable by any
   * operator while this whole file stays green. Mounted through the row — the way
   * `HostsScreenSshAgentAcceptance` mounts it — that line is what is under test.
   */
  it("offers the add on the row of a host whose agent is reachable", () => {
    // Given a host whose agent answered and is holding nothing
    const backend = aBackendThatAdds(aHostPromptFeed());

    // When the whole ssh-agent cell of its row renders
    mountRow(anEmptyAgent(), backend);

    // Then the add is there to be used, not merely implemented: somewhere to name a key, and
    // something to press. (The button itself stays disabled until a key is named — the same state
    // the action-level test above asserts.)
    addKey.action(HOST).should("exist");
    addKey.keyField(HOST).should("be.enabled");
    addKey.start(HOST).should("exist");
  });

  it("asks the host to load the key the operator named", () => {
    // Given a host that will accept the add
    const feed = aHostPromptFeed();
    const backend = aBackendThatAdds(feed);
    mountAction(anEmptyAgent(), backend);

    // When the operator names a key and starts the add
    addKey.addKey(HOST, KEY_PATH);

    // Then the daemon is asked for that key, on that host. Without the subject the daemon would be
    // guessing which key the operator meant, and the dialog would name a key nobody chose.
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      const calls = b.callsTo(ConnectionService.method.addHostKey);
      expect(calls).to.have.length(1);
      expect(calls[0].subject).to.equal(KEY_PATH);
      expect(calls[0].daemonInstanceId).to.equal(HOST);
    });
  });

  it("raises the passphrase dialog for the key the host asks about", () => {
    // Given an add in flight, with the host blocked on an answer
    const feed = aHostPromptFeed();
    mountAction(anEmptyAgent(), aBackendAwaitingAnAnswer(feed));

    // When the host raises its passphrase question, publishing its key
    theHostAsksForAPassphrase(feed, hostPublicKey, hostKeyFingerprint);

    // Then the operator sees the dialog, naming the host, the key and the fingerprint to verify —
    // the fingerprint of the key that arrived, which is the one their passphrase is encrypted under
    dialog.root(HOST).should("contain.text", KEY_PATH).and("contain.text", hostKeyFingerprint);
  });

  it("confirms the key the agent is now holding once the add succeeds", () => {
    // Given a host that accepts the add and reports the identity it took
    const backend = aBackendThatAdds(aHostPromptFeed());
    mountAction(anEmptyAgent(), backend);

    // When the operator adds a key
    addKey.addKey(HOST, KEY_PATH);

    // Then the fingerprint the agent reported is shown, so the operator can match it against the
    // key list beside it rather than taking "done" on trust.
    addKey.addedConfirmation(HOST).should("contain.text", ADDED_FINGERPRINT);
  });
});

// ---------------------------------------------------------------------------
// The key the answer is encrypted under, versus the fingerprint beside it
// ---------------------------------------------------------------------------

/**
 * A prompt carries a key *and* a fingerprint string, and only the key encrypts anything. Both ride
 * the common room, which the trust model calls a trusted peer group and **not** an authenticated
 * one — so the string is a claim, and a public one an active peer can replay unchanged next to its
 * own key. These tests are the browser refusing to take that claim on trust.
 */
describe("Hosts screen add key — the key a prompt actually carries", () => {
  before(() => {
    cy.wrap(anRsaOaepPublicKey())
      .then((spkiDer) => {
        hostPublicKey = spkiDer as unknown as Uint8Array;
        return fingerprintOf(hostPublicKey);
      })
      .then((fingerprint) => {
        hostKeyFingerprint = fingerprint as unknown as string;
        return anRsaOaepPublicKey();
      })
      .then((spkiDer) => {
        aPeersPublicKey = spkiDer as unknown as Uint8Array;
      });
  });

  beforeEach(() => {
    cy.clearLocalStorage();
  });

  it("refuses a prompt whose key is not the key its fingerprint describes", () => {
    // Given an add in flight on a host this browser has never seen
    const feed = aHostPromptFeed();
    mountAction(anEmptyAgent(), aBackendAwaitingAnAnswer(feed));

    // When the prompt arrives carrying one key while advertising the fingerprint of another
    theHostAsksForAPassphrase(feed, aPeersPublicKey, hostKeyFingerprint);

    // Then nothing may be sent. A host describing its own key gets it right, so the two halves
    // disagreeing is a frame that was rewritten in flight — not a first sighting to be pinned.
    dialog.mismatchWarning().should("exist");
    dialog.submit().should("be.disabled");
  });

  it("catches this hosts own fingerprint replayed beside another key", () => {
    // Given this browser pinned the host's key on an earlier prompt it answered
    const feed = aHostPromptFeed();
    mountAction(anEmptyAgent(), aBackendAwaitingAnAnswer(feed));
    hostKeyPins.pin(HOST, hostKeyFingerprint);

    // When a peer replays that fingerprint — public, non-secret, and the very string the operator
    // verified out of band — beside its own key
    theHostAsksForAPassphrase(feed, aPeersPublicKey, hostKeyFingerprint);

    // Then the substitution is caught rather than reading as the key that was pinned. This is the
    // whole of what pinning buys: the pin records the key that encrypts, so a replayed string
    // cannot stand in for it.
    dialog.mismatchWarning().should("exist");
    dialog.submit().should("be.disabled");
  });
});

// ---------------------------------------------------------------------------
// A host key that legitimately rotated
// ---------------------------------------------------------------------------

/**
 * A pin that can never be replaced locks the operator out: a host that regenerated
 * `host-prompt-key.pem` blocks every prompt from then on, with no remedy short of clearing browser
 * storage. The PRD's committed design is that a changed key blocks **and the operator can accept
 * the new one explicitly**.
 */
describe("Hosts screen add key — accepting a rotated host key", () => {
  before(() => {
    cy.wrap(anRsaOaepPublicKey())
      .then((spkiDer) => {
        hostPublicKey = spkiDer as unknown as Uint8Array;
        return fingerprintOf(hostPublicKey);
      })
      .then((fingerprint) => {
        hostKeyFingerprint = fingerprint as unknown as string;
      });
  });

  beforeEach(() => {
    cy.clearLocalStorage();
  });

  it("blocks a changed host key until the operator accepts it", () => {
    // Given this browser pinned a different key for the host, which has since regenerated its own
    const feed = aHostPromptFeed();
    mountAction(anEmptyAgent(), aBackendAwaitingAnAnswer(feed));
    hostKeyPins.pin(HOST, A_PREVIOUSLY_PINNED_KEY);
    theHostAsksForAPassphrase(feed, hostPublicKey, hostKeyFingerprint);
    dialog.changedWarning().should("exist");

    // When the operator states they verified the new key with the host, and accepts it
    dialog.acceptChangedKey();
    dialog.input().type(PASSPHRASE);

    // Then the answer can go — the operator has a way through a legitimate rotation
    dialog.submit().should("not.be.disabled");
  });

  it("refuses to accept a changed key on the accept button alone", () => {
    // Given a changed key
    const feed = aHostPromptFeed();
    mountAction(anEmptyAgent(), aBackendAwaitingAnAnswer(feed));
    hostKeyPins.pin(HOST, A_PREVIOUSLY_PINNED_KEY);

    // When the prompt arrives
    theHostAsksForAPassphrase(feed, hostPublicKey, hostKeyFingerprint);

    // Then accepting takes a deliberate statement first. A one-click dismissal beside a warning is
    // a warning nobody reads, and this is the one moment the operator is the only check there is.
    dialog.acceptChangedKeyButton().should("be.disabled");
    dialog.submit().should("be.disabled");
  });

  it("records the accepted key as the pin the next prompt is compared against", () => {
    // Given a changed key the operator is looking at
    const feed = aHostPromptFeed();
    mountAction(anEmptyAgent(), aBackendAwaitingAnAnswer(feed));
    hostKeyPins.pin(HOST, A_PREVIOUSLY_PINNED_KEY);
    theHostAsksForAPassphrase(feed, hostPublicKey, hostKeyFingerprint);

    // When they accept it
    dialog.acceptChangedKey();

    // Then the new key is pinned outright, not the old pin merely dropped: dropping it would
    // silently accept whichever key turned up next, which is the sighting nobody looked at
    hostKeyPins.expectPinned(HOST, hostKeyFingerprint);
  });
});

// ---------------------------------------------------------------------------
// What the host said about the answer
// ---------------------------------------------------------------------------

/** A daemon that takes the answer and refuses it, the way an expired prompt is refused. */
function aBackendRefusingTheAnswer(feed: HostPromptFeed, rejectionReason: string) {
  return anInMemoryRpcBackend().implement(ConnectionService, {
    ...feed.handlers,
    addHostKey: () => new Promise(() => undefined),
    answerHostPrompt: async () => ({ accepted: false, rejectionReason }),
  });
}

describe("Hosts screen add key — a refused answer", () => {
  before(() => {
    cy.wrap(anRsaOaepPublicKey())
      .then((spkiDer) => {
        hostPublicKey = spkiDer as unknown as Uint8Array;
        return fingerprintOf(hostPublicKey);
      })
      .then((fingerprint) => {
        hostKeyFingerprint = fingerprint as unknown as string;
      });
  });

  beforeEach(() => {
    cy.clearLocalStorage();
  });

  it("tells the operator when the host refused the answer, and why", () => {
    // Given a host that will refuse the answer because its prompt expired
    const feed = aHostPromptFeed();
    mountAction(
      anEmptyAgent(),
      aBackendRefusingTheAnswer(feed, "this prompt expired before the answer arrived"),
    );
    theHostAsksForAPassphrase(feed, hostPublicKey, hostKeyFingerprint);

    // When the operator answers it
    dialog.input().type(PASSPHRASE);
    dialog.submit().click();

    // Then the refusal reaches them. The dialog is already gone by the time the host answers, so a
    // response nobody reads leaves an operator watching a row that will never say anything again.
    hostAddKeyOutcome.saying(HOST, /expired/i);
  });

  it("ends the add when the operator cancels the dialog, so the row can be used again", () => {
    // Given a host blocked on an answer — `AddHostKey` does not return until the prompt is resolved
    const feed = aHostPromptFeed();
    mountAction(anEmptyAgent(), aBackendAwaitingAnAnswer(feed));
    theHostAsksForAPassphrase(feed, hostPublicKey, hostKeyFingerprint);

    // When the operator cancels instead of answering
    dialog.cancel().click();

    // Then the add is over here and the control comes back. Leaving it disabled for the prompt's
    // full lifetime would look like a hung row, with nothing on screen explaining the wait.
    addKey.start(HOST).should("be.enabled");
    addKey.keyField(HOST).should("be.enabled");
    hostAddKeyOutcome.saying(HOST, /cancel/i);
  });
});
