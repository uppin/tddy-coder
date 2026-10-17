/**
 * Acceptance tests: the passphrase prompt a host raises while a session start is cloning its
 * codebase, shown over the create-session form instead of on the Hosts screen.
 *
 * The prompt itself, its encryption and the key-continuity check are the Hosts screen's and are
 * pinned in `HostAddKeyAcceptance.cy.tsx`; what is pinned here is that the *start* surfaces them:
 * only while a start is in flight, for the host that is cloning, with the same guarantees.
 *
 * Feature: docs/ft/daemon/clone-ssh-passphrase.md
 */

import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  HostService,
  type AnswerHostPromptRequest,
} from "../../src/gen/host_pb";
import { StartPassphrasePrompt } from "../../src/components/sessions/StartPassphrasePrompt";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aHostPromptFeed, type HostPromptFeed } from "../support/rpc/hostPromptFeed";
import {
  anRsaOaepKeypair,
  fingerprintOf,
  hostKeyPins,
  type AHostPromptKeypair,
} from "../support/hostKeys";
import { hostPassphraseDialogPage as dialog } from "../support/pages/hostsScreenPage";

const CLONING_HOST = "udoo";
const PASSPHRASE = "correct horse battery staple";
const KEY_ON_THE_HOST = "/home/mantas/.ssh/id_rsa";

let hostKeypair: AHostPromptKeypair;
let hostFingerprint: string;

/** The daemon on the cloning host, recording the answers it is sent. */
function aCloningHost(feed: HostPromptFeed, answers: AnswerHostPromptRequest[]): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(HostService, {
    ...feed.handlers,
    answerHostPrompt: async (request: AnswerHostPromptRequest) => {
      answers.push(request);
      return { accepted: true, rejectionReason: "" };
    },
  });
}

const SESSION_TOKEN = "session-token-of-the-operator";

function mountStart(backend: InMemoryRpcBackend, props: { active: boolean }) {
  mountWithRpc(
    withSelectedDaemon(
      <StartPassphrasePrompt
        instanceId={CLONING_HOST}
        active={props.active}
        sessionToken={SESSION_TOKEN}
      />,
      [{ instanceId: CLONING_HOST, label: CLONING_HOST }],
    ),
    backend,
  );
}

/** Retries until the dialog's component has subscribed to the cloning host's prompt feed. */
function waitForTheStreamToOpen(feed: HostPromptFeed) {
  cy.wrap(null).should(() => expect(feed.subscriptionCount()).to.equal(1));
}

/** The clone on `CLONING_HOST` asks for the passphrase of the key at `KEY_ON_THE_HOST`. */
function theCloneAsksForAPassphrase(feed: HostPromptFeed, promptId = "prompt-1") {
  // The stream opens asynchronously after mount; a prompt raised before it does reaches nobody.
  waitForTheStreamToOpen(feed);
  cy.then(() =>
    feed.raise({
      promptId,
      daemonInstanceId: CLONING_HOST,
      subject: KEY_ON_THE_HOST,
      hostPublicKey: hostKeypair.spkiDer,
      hostPublicKeyFingerprint: hostFingerprint,
    }),
  );
}

describe("Passphrase prompt during a session start", () => {
  before(() => {
    cy.wrap(anRsaOaepKeypair()).then(async (keypair) => {
      hostKeypair = keypair as unknown as AHostPromptKeypair;
      hostFingerprint = await fingerprintOf(hostKeypair.spkiDer);
    });
  });

  beforeEach(() => {
    cy.clearLocalStorage();
  });

  it("shows the passphrase dialog when the cloning host raises a prompt during a start", () => {
    // Given a start in flight against a host that is cloning
    const feed = aHostPromptFeed();
    mountStart(aCloningHost(feed, []), { active: true });
    waitForTheStreamToOpen(feed);

    // When that host asks for a key's passphrase
    theCloneAsksForAPassphrase(feed);

    // Then the operator is asked, by host and by key
    dialog.root(CLONING_HOST).should("contain.text", CLONING_HOST);
    dialog.root(CLONING_HOST).should("contain.text", KEY_ON_THE_HOST);
  });

  it("subscribes to nothing and shows nothing while no start is in flight", () => {
    // Given a form with no start under way
    const feed = aHostPromptFeed();
    mountStart(aCloningHost(feed, []), { active: false });

    // Then no prompt stream was opened — a screen nobody is starting from must cost no stream
    cy.then(() => expect(feed.subscriptionCount()).to.equal(0));
    dialog.root(CLONING_HOST).should("not.exist");
  });

  it("encrypts the answer under the hosts published key and sends it to the cloning host", () => {
    // Given the dialog is open for a prompt
    const feed = aHostPromptFeed();
    const answers: AnswerHostPromptRequest[] = [];
    mountStart(aCloningHost(feed, answers), { active: true });
    theCloneAsksForAPassphrase(feed);

    // When the operator types the passphrase
    dialog.input().type(PASSPHRASE);
    dialog.submit().click();

    // Then exactly one answer reached the host, naming the prompt, readable only with the host's
    // private key, and containing the passphrase nowhere in the clear
    cy.wrap(null).should(() => expect(answers).to.have.length(1));
    cy.then(async () => {
      expect(answers[0].promptId).to.equal("prompt-1");
      expect(answers[0].sessionToken, "answered as the operator who started the clone").to.equal(SESSION_TOKEN);
      expect(new TextDecoder().decode(answers[0].encryptedAnswer)).to.not.contain(PASSPHRASE);
      expect(await hostKeypair.decrypt(answers[0].encryptedAnswer)).to.equal(PASSPHRASE);
    });
  });

  it("clears the dialog once the prompt is answered", () => {
    const feed = aHostPromptFeed();
    mountStart(aCloningHost(feed, []), { active: true });
    theCloneAsksForAPassphrase(feed);

    dialog.input().type(PASSPHRASE);
    dialog.submit().click();

    dialog.root(CLONING_HOST).should("not.exist");
  });

  it("clears the dialog when the operator cancels, without sending an answer", () => {
    const feed = aHostPromptFeed();
    const answers: AnswerHostPromptRequest[] = [];
    mountStart(aCloningHost(feed, answers), { active: true });
    theCloneAsksForAPassphrase(feed);

    dialog.cancel().click();

    dialog.root(CLONING_HOST).should("not.exist");
    cy.then(() => expect(answers, "cancelling answers nothing").to.have.length(0));
  });

  it("clears the dialog when the start ends without the prompt being answered", () => {
    // Given a prompt on screen for a start that is then abandoned
    const feed = aHostPromptFeed();
    const backend = aCloningHost(feed, []);
    mountStart(backend, { active: true });
    theCloneAsksForAPassphrase(feed);
    dialog.root(CLONING_HOST).should("exist");

    // When the start is no longer in flight
    mountStart(backend, { active: false });

    // Then there is no dialog left asking for an answer nobody is waiting for
    dialog.root(CLONING_HOST).should("not.exist");
  });

  it("refuses to take an answer when the hosts public key is not the one pinned for it", () => {
    // Given this browser pinned a different key for the host earlier
    hostKeyPins.pin(CLONING_HOST, "SHA256:9WK1EJ1YHXbCP9V0Y13uwbHFuqWFcAe1eFf0kSPn5Ok");
    const feed = aHostPromptFeed();
    const answers: AnswerHostPromptRequest[] = [];
    mountStart(aCloningHost(feed, answers), { active: true });

    // When the host now presents another
    theCloneAsksForAPassphrase(feed);

    // Then the dialog warns and the answer is blocked — the substitution the pin exists to catch
    dialog.changedWarning().should("exist");
    dialog.submit().should("be.disabled");
    cy.then(() => expect(answers).to.have.length(0));
  });
});
