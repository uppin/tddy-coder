/**
 * Acceptance tests: opening a host's desktop from its row.
 *
 * The gating tests are the substance. This action cannot work without the `media` capability — a
 * frame pipe carries no video — so following `InspectorTabs` the control is **absent**, not
 * disabled. Asserting "not.exist" rather than "be.disabled" is the difference between a UI that is
 * honest about what a transport can do and one that offers something it cannot deliver.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-06-desktop-connect.md
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AuthService } from "../../src/gen/auth_pb";
import {
  HostPromptKind,
  HostRemoteDesktopSchema,
  HostService,
  ProbeOutcome,
} from "../../src/gen/host_pb";
import { ScreenSharingService } from "../../src/gen/screen_sharing_pb";
import { HostRowRemoteDesktop } from "../../src/components/hosts/HostRowRemoteDesktop";
import { HostRowTooling } from "../../src/components/hosts/HostRowTooling";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aHostConnection, aRegistryServing } from "../support/rpc/hostConnections";
import {
  aHostPromptFeed,
  type HostPromptFeed,
  type HostPromptFrame,
} from "../support/rpc/hostPromptFeed";
import {
  ACCESS_TOKEN_KEY,
  CURRENT_ACCESS_TOKEN,
  REFRESH_TOKEN_KEY,
  VALID_REFRESH_TOKEN,
} from "../support/rpc/durableSessionBackend";
import { aGitHubUser } from "../support/rpc/responses";
import { anRsaOaepKeypair, type AHostPromptKeypair } from "../support/hostKeys";
import { ConnectionProviders } from "../../src/rpc/connections/registry";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../src/hooks/authProvider";
import { useHostPrompts } from "../../src/rpc/useHostPrompts";
import { hostDesktopPage, hostPassphraseDialogPage } from "../support/pages/hostsScreenPage";

const HOST = "workstation-1";
const VNC = 1;

function aReachableVnc() {
  return create(HostRemoteDesktopSchema, {
    outcome: ProbeOutcome.OK,
    protocol: VNC,
    canBridge: true,
    desktopReachable: true,
    port: 5900,
  });
}

/** Mounts the row section with a daemon list that carries media (the LiveKit path). */
function mountWithMedia(
  readings = [aReachableVnc()],
  backend: InMemoryRpcBackend = anInMemoryRpcBackend(),
) {
  mountWithRpc(
    withSelectedDaemon(<HostRowRemoteDesktop instanceId={HOST} readings={readings} />),
    backend,
  );
}

/**
 * Mounts the row over a wire the test **states**, rather than one a fixture implies.
 *
 * `withSelectedDaemon` hands the tree a `Room`, and a room makes the LiveKit provider claim every
 * host with all three capabilities — so it cannot express the `{"rpc"}`-only half of a gating
 * scenario at all. `room={null}` plus a registry built from `aHostConnection` makes this
 * connection the only answer, which is the difference between asserting the gate and constructing
 * it. See `cypress/support/rpc/hostConnections.ts`.
 */
function mountOnWire(carriesMedia: boolean) {
  const backend = anInMemoryRpcBackend();
  const host = carriesMedia
    ? aHostConnection(HOST).reachedOverLiveKit().servingOver(backend.transport()).build()
    : aHostConnection(HOST).servingOver(backend.transport()).build();
  mountWithRpc(
    <AuthProvider>
      <ConnectionProviders registry={aRegistryServing(host)}>
        <SelectedDaemonProvider
          room={null}
          daemons={[{ instanceId: HOST, label: HOST }]}
          servingInstanceId={HOST}
        >
          <HostRowRemoteDesktop instanceId={HOST} readings={[aReachableVnc()]} />
        </SelectedDaemonProvider>
      </ConnectionProviders>
    </AuthProvider>,
    backend,
  );
}

describe("Host desktop connect", () => {
  it("offers a connect action for a reachable host on a media carrying connection", () => {
    // Given a host whose desktop is reachable and which tddy can bridge
    mountWithMedia();

    // When the row is rendered
    // Then it offers a way in
    hostDesktopPage.connect(HOST).should("exist");
  });

  it("offers connect for a reachable desktop but not for an unreachable one", () => {
    // Given a reachable desktop
    mountWithMedia();

    // Then the action is offered
    hostDesktopPage.connect(HOST).should("exist");

    // When the same host reports no desktop serving
    mountWithMedia([
      create(HostRemoteDesktopSchema, {
        outcome: ProbeOutcome.OK,
        protocol: VNC,
        canBridge: true,
        desktopReachable: false,
        port: 5900,
      }),
    ]);

    // Then it is withdrawn — asserted against the positive case, so an action that never renders
    // at all cannot satisfy this test.
    hostDesktopPage.connect(HOST).should("not.exist");
  });

  it("offers connect when tddy can bridge but not when the bridge is missing", () => {
    // Given a host that can bridge a reachable desktop
    mountWithMedia();
    hostDesktopPage.connect(HOST).should("exist");

    // When the same desktop is reachable but this daemon has no bridge binary
    mountWithMedia([
      create(HostRemoteDesktopSchema, {
        outcome: ProbeOutcome.OK,
        protocol: VNC,
        canBridge: false,
        desktopReachable: true,
        port: 5900,
      }),
    ]);

    // Then connecting is not offered: a reachable desktop tddy cannot stream is still not connectable
    hostDesktopPage.connect(HOST).should("not.exist");
  });

  it("offers connect over a media carrying connection but not over one without media", () => {
    // Given the same reachable desktop reached over a wire that carries media
    mountOnWire(true);
    hostDesktopPage.connect(HOST).should("exist");

    // When the only wire reaching that host is a frame pipe, which carries rpc and nothing else
    mountOnWire(false);

    // Then the action is absent rather than disabled — a video track genuinely cannot arrive over a
    // frame pipe, and offering a control that cannot work is worse than not offering one.
    hostDesktopPage.connect(HOST).should("not.exist");
  });

  it("opens the overlay for the selected host", () => {
    // Given a connectable host
    mountWithMedia();

    // When the operator connects
    hostDesktopPage.connect(HOST).click();

    // Then that host's desktop opens in the existing overlay
    hostDesktopPage.overlay(HOST).should("exist");
  });
});


// ---------------------------------------------------------------------------
// AC-7 — the desktop password, prompted and never kept
// ---------------------------------------------------------------------------

/**
 * The password of a host desktop is never stored anywhere, so opening one has to **ask**.
 *
 * The question is raised by the daemon, and it has to be: `HostPromptEvent` is the only place a
 * host's public key is published, and it carries the fingerprint the client pins — so a browser
 * that has not been asked anything has nothing to encrypt under. That is why this spec drives the
 * prompt feed rather than typing into a dialog the row opened by itself.
 *
 * The load-bearing assertion is the one about what goes back on the wire. A test that only checked
 * "a dialog appeared and the operator typed" would pass just as well against a component that put
 * the password in the clear on `AnswerHostPrompt` — or on any other call this screen makes to the
 * host, which is why the search covers every one of them rather than the answer alone.
 */

const A_PROMPT = "prompt-desktop-1";
const THE_DESKTOP = "Desktop on workstation-1:5900";
const THE_DESKTOP_PASSWORD = "correct horse battery staple";
const HOST_KEY_FINGERPRINT = "SHA256:ZLBiCcwTvIcQUyRnvSHhpsdgWLLLZtWbBAPtgWNBpAg";
const A_HOST_TARGET = "target-1";

/** The other question this same per-host feed carries — `HostAddKeyAction`'s, for a key on it. */
const AN_SSH_KEY_PROMPT = "prompt-ssh-key-1";
const A_KEY_ON_THAT_HOST = "/home/ada/.ssh/id_ed25519";

/**
 * The host's prompt keypair: the half it publishes with a prompt, and the half only it holds.
 *
 * Generated for real, exactly as `HostAddKeyAcceptance` does and for the same reason: a stubbed
 * `crypto.subtle` would hollow out the one assertion carrying this test's security claim.
 *
 * The **private** half is kept, and that is what makes an answer checkable. RSA-2048 OAEP output is
 * 256 bytes whatever it encrypts, so a size assertion holds identically for the typed password, for
 * the empty string, and for a constant a dialog that never read its own field would send — which is
 * precisely the failure AC-7 exists to rule out. Only decrypting says what was sent.
 */
let hostKey: AHostPromptKeypair;
/** The SPKI DER a daemon publishes with a prompt — the public half of {@link hostKey}. */
let hostPublicKey: Uint8Array;

/** Generate this suite's host key. Used as a `before`, once per describe that raises a prompt. */
function givenTheHostsPromptKey() {
  cy.wrap(anRsaOaepKeypair()).then((keypair) => {
    hostKey = keypair as unknown as AHostPromptKeypair;
    hostPublicKey = hostKey.spkiDer;
  });
}

/** The desktop's own question, as the host raises it. */
function theDesktopPasswordQuestion(): HostPromptFrame {
  return {
    promptId: A_PROMPT,
    daemonInstanceId: HOST,
    kind: HostPromptKind.DESKTOP_PASSWORD,
    subject: THE_DESKTOP,
    hostPublicKey,
    hostPublicKeyFingerprint: HOST_KEY_FINGERPRINT,
  };
}

/** A key passphrase question, on the same host and the same feed — and not this surface's. */
function aKeyPassphraseQuestion(): HostPromptFrame {
  return {
    promptId: AN_SSH_KEY_PROMPT,
    daemonInstanceId: HOST,
    kind: HostPromptKind.SSH_KEY_PASSPHRASE,
    subject: A_KEY_ON_THAT_HOST,
    hostPublicKey,
    hostPublicKeyFingerprint: HOST_KEY_FINGERPRINT,
  };
}

/**
 * A daemon that takes the start and then stays blocked on the password — the state an operator is
 * actually looking at while the dialog is up.
 */
function aDaemonAwaitingADesktopPassword(feed: HostPromptFeed): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .implement(HostService, {
      ...feed.handlers,
      answerHostPrompt: async () => ({ accepted: true, rejectionReason: "" }),
    })
    .implement(ScreenSharingService, {
      listHostTargets: async () => ({ targets: [] }),
      addHostTarget: async () => ({ targetId: A_HOST_TARGET }),
      // Never settles: the host has raised its question and is waiting on the answer, which is the
      // whole point of prompting rather than storing.
      startHostStream: () => new Promise(() => undefined),
      stopHostStream: async () => ({ ok: true }),
    });
}

/** Everything the browser sent this host, as one string to search for a leaked secret in. */
function everythingSentTo(backend: InMemoryRpcBackend): string {
  const sent = [
    ...backend.callsTo(HostService.method.answerHostPrompt),
    ...backend.callsTo(ScreenSharingService.method.startHostStream),
    ...backend.callsTo(ScreenSharingService.method.addHostTarget),
  ];
  return JSON.stringify(sent, (_key, value) =>
    value instanceof Uint8Array ? new TextDecoder().decode(value) : value,
  );
}

/** Everything this browser kept, across both per-origin stores. */
function everythingStoredBy(win: Cypress.AUTWindow): string {
  const read = (store: Storage) =>
    Object.keys(store)
      .map((key) => `${key}=${store.getItem(key) ?? ""}`)
      .join("\n");
  return `${read(win.localStorage)}\n${read(win.sessionStorage)}`;
}

/**
 * What the host — and only the host — reads out of the single answer the browser sent it.
 *
 * Yields the plaintext, so a spec asserts the password that was typed rather than the size of the
 * block it came in. Decryption is done with the private half the prompt's key was generated with,
 * so a payload encrypted for anything else fails here by name (see `cypress/support/hostKeys.ts`).
 */
function theAnswerTheHostCanRead(backend: InMemoryRpcBackend): Cypress.Chainable<string> {
  return cy
    .wrap(backend)
    .should((b: InMemoryRpcBackend) =>
      expect(
        b.callsTo(HostService.method.answerHostPrompt),
        "exactly one answer must reach the host that raised the prompt",
      ).to.have.length(1),
    )
    .then((b: InMemoryRpcBackend) =>
      hostKey.decrypt(b.callsTo(HostService.method.answerHostPrompt)[0].encryptedAnswer),
    );
}

describe("Host desktop password", () => {
  before(givenTheHostsPromptKey);

  beforeEach(() => {
    cy.clearLocalStorage();
  });

  it("prompts for a desktop password without persisting it", () => {
    // Given an open host desktop whose daemon is blocked on the password
    const feed = aHostPromptFeed();
    const backend = aDaemonAwaitingADesktopPassword(feed);
    mountWithMedia([aReachableVnc()], backend);
    hostDesktopPage.connect(HOST).click();

    // …and the browser listening to that host's questions, or the host is asking nobody
    cy.wrap(feed).should((f: HostPromptFeed) => expect(f.subscriptionCount()).to.equal(1));

    // When the host asks for that desktop's password
    cy.then(() => {
      feed.raise(theDesktopPasswordQuestion());
    });

    // Then the operator is asked, for the desktop the host named and under the key to verify
    hostPassphraseDialogPage
      .root(HOST)
      .should("contain.text", THE_DESKTOP)
      .and("contain.text", HOST_KEY_FINGERPRINT);

    // When they answer
    hostPassphraseDialogPage.input().type(THE_DESKTOP_PASSWORD);
    hostPassphraseDialogPage.submit().click();

    // Then exactly one answer went back to the host that asked. Asserted before anything about
    // absence: a password that never left is trivially not in a request and not in storage, and an
    // assertion that holds for the wrong reason reports the property as verified for ever.
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      const answers = b.callsTo(HostService.method.answerHostPrompt);
      expect(answers, "the answer must reach the host that raised the prompt").to.have.length(1);
      expect(answers[0].promptId).to.equal(A_PROMPT);
      expect(answers[0].daemonInstanceId).to.equal(HOST);
    });

    // …and what that answer says, read as only this host can read it, is the password the operator
    // typed. A dialog that encrypted a constant, or the empty string, produces a ciphertext of
    // exactly the same size and fails only here.
    theAnswerTheHostCanRead(backend).should("equal", THE_DESKTOP_PASSWORD);

    // …and nothing the browser sent this host carries the password in the clear — not the answer,
    // and not the start or the target it was opened through.
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      expect(
        everythingSentTo(b),
        "the desktop password must never leave the browser in the clear",
      ).to.not.contain(THE_DESKTOP_PASSWORD);
    });

    // …and nothing kept it: not persisted is the whole reason this node prompts rather than
    // reusing the per-session vault.
    cy.window().should((win) => {
      expect(everythingStoredBy(win), "a prompted password is used once and dropped").to.not.contain(
        THE_DESKTOP_PASSWORD,
      );
    });

    // …and it is not left on screen either.
    hostPassphraseDialogPage.input().should("have.value", "");
  });
});

// ---------------------------------------------------------------------------
// AC-2 — the desktop that has no password at all
// ---------------------------------------------------------------------------

/**
 * Plenty of desktops have no password, and the host asks anyway.
 *
 * The prompt is raised on **every** start because the host cannot know in advance whether the
 * desktop it is about to open wants a password. So an operator whose desktop has none is asked a
 * question whose only truthful answer is nothing at all — and a dialog that refuses an empty answer
 * refuses them the desktop itself, which is AC-2 gone for that entire class of host.
 *
 * The opposite mistake — taking an empty answer where one cannot possibly be right — is pinned on
 * the other flow this same dialog serves, by `HostAddKeyAcceptance`'s "refuses to send an empty
 * passphrase, which unlocks no key". The two together are what stop this from being satisfied by a
 * dialog that simply always accepts.
 */

const A_LIVEKIT_ROOM = "host-desktop-workstation-1";
const A_LIVEKIT_URL = "wss://livekit.example.test";
const THE_BRIDGE = "bridge-workstation-1";
const THE_TRACK = "desktop";

/**
 * A daemon whose start is released by the answer, and by nothing else.
 *
 * That ordering is the whole fixture: `StartHostStream` settles only once `AnswerHostPrompt` has
 * arrived, exactly as a live host's does — the prompt blocks the spawn. It is what makes a desktop
 * on screen evidence that the empty answer got through, rather than evidence that a stub resolved
 * on its own.
 */
function aDesktopReleasedByTheAnswer(feed: HostPromptFeed): InMemoryRpcBackend {
  let theAnswerArrived: () => void = () => undefined;
  const answered = new Promise<void>((resolve) => {
    theAnswerArrived = resolve;
  });
  return anInMemoryRpcBackend()
    .implement(HostService, {
      ...feed.handlers,
      answerHostPrompt: async () => {
        theAnswerArrived();
        return { accepted: true, rejectionReason: "" };
      },
    })
    .implement(ScreenSharingService, {
      listHostTargets: async () => ({ targets: [] }),
      addHostTarget: async () => ({ targetId: A_HOST_TARGET }),
      startHostStream: async () => {
        await answered;
        return {
          livekitRoom: A_LIVEKIT_ROOM,
          livekitUrl: A_LIVEKIT_URL,
          bridgeIdentity: THE_BRIDGE,
          trackName: THE_TRACK,
          width: 1920,
          height: 1080,
        };
      },
      stopHostStream: async () => ({ ok: true }),
    });
}

/** Open the desktop and arrive at the moment the host has asked and is waiting on the answer. */
function connectAndBeAskedFor(backend: InMemoryRpcBackend, feed: HostPromptFeed) {
  mountWithMedia([aReachableVnc()], backend);
  hostDesktopPage.connect(HOST).click();
  // …with the browser listening to that host's questions, or the host is asking nobody
  cy.wrap(feed).should((f: HostPromptFeed) => expect(f.subscriptionCount()).to.equal(1));
  cy.then(() => {
    feed.raise(theDesktopPasswordQuestion());
  });
}

describe("Host desktop without a password", () => {
  before(givenTheHostsPromptKey);

  beforeEach(() => {
    cy.clearLocalStorage();
  });

  it("opens a desktop that has no password from an empty answer", () => {
    // Given a host asking for the password of a desktop that does not have one
    const feed = aHostPromptFeed();
    const backend = aDesktopReleasedByTheAnswer(feed);
    connectAndBeAskedFor(backend, feed);

    // When the operator sends without typing one
    hostPassphraseDialogPage.submit().click();

    // Then that answer reached the host that asked, and reached it encrypted: an empty password is
    // still a password, and a dialog that shortcut the encryption for it would be sending plaintext
    // on the one call this node exists to keep secret.
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      const answers = b.callsTo(HostService.method.answerHostPrompt);
      expect(answers, "the empty answer must reach the host that raised the prompt").to.have.length(
        1,
      );
      expect(answers[0].promptId).to.equal(A_PROMPT);
    });

    // …and what the host reads out of it is the empty answer itself, not some stand-in the dialog
    // substituted. Every ciphertext under this key is 256 bytes, so this is the only assertion that
    // tells the no-password case apart from the password one at all.
    theAnswerTheHostCanRead(backend).should("equal", "");

    // …and the desktop the question was blocking opened. The start settles on the answer and on
    // nothing else, so a stream on screen is that empty answer accepted end to end.
    hostDesktopPage.stream().should("exist");
  });

  it("says the answer may be left blank when the desktop has no password", () => {
    // Given a host asking for the password of a desktop that does not have one
    const feed = aHostPromptFeed();
    connectAndBeAskedFor(aDesktopReleasedByTheAnswer(feed), feed);

    // Then the dialog says so, rather than leaving an operator to guess that nothing is an answer
    // and invent a password the desktop never had. Matched as a pattern: what has to be true is
    // that blankness is named, not which sentence names it.
    hostPassphraseDialogPage.root(HOST).invoke("text").should("match", /blank/i);
  });
});

// ---------------------------------------------------------------------------
// The session every host-scoped call is gated on
// ---------------------------------------------------------------------------

/**
 * A daemon refuses any host-scoped call that arrives without the operator's session token.
 *
 * The browser's auth gate (`src/rpc/authGatedTransport.ts`, wrapped around every LiveKit-reached
 * host) rewrites `sessionToken` on the way out — but only on a request that **already carries the
 * field**, since it tests `"sessionToken" in message`. A call built without it is therefore not
 * repaired in transit: it arrives unauthenticated and is rejected. None of that shows up on the
 * local HTTP path, where the daemon serving the page answers anyway, which is how a desktop that
 * cannot open a single *remote* host passes a screenful of green row tests.
 *
 * So this is asserted on the wire, on the calls themselves, rather than on anything visible.
 */
function alsoServingTheOperatorsSession(backend: InMemoryRpcBackend): InMemoryRpcBackend {
  return backend.implement(AuthService, {
    getAuthStatus: async (req) =>
      req.sessionToken === CURRENT_ACCESS_TOKEN
        ? { authenticated: true, user: aGitHubUser() }
        : { authenticated: false, user: undefined },
  });
}

/** A browser holding a live session, as a signed-in operator's browser holds one. */
function aSignedInOperator() {
  cy.window().then((win) => {
    win.localStorage.setItem(ACCESS_TOKEN_KEY, CURRENT_ACCESS_TOKEN);
    win.localStorage.setItem(REFRESH_TOKEN_KEY, VALID_REFRESH_TOKEN);
  });
}

describe("Host desktop calls the daemon authenticates", () => {
  before(givenTheHostsPromptKey);

  beforeEach(() => {
    cy.clearLocalStorage();
  });

  it("carries the operators session on every call it makes to the host", () => {
    // Given a signed-in operator and a connectable host
    aSignedInOperator();
    const feed = aHostPromptFeed();
    const backend = alsoServingTheOperatorsSession(aDaemonAwaitingADesktopPassword(feed));
    mountWithMedia([aReachableVnc()], backend);

    // When they open that host's desktop
    hostDesktopPage.connect(HOST).click();

    // Then every call the overlay made on the way — reading this host's targets, adding the one for
    // the probed endpoint, and starting the stream on it — reached the host on that session, rather
    // than with the empty default a request omitting the field arrives with.
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      const [listed] = b.callsTo(ScreenSharingService.method.listHostTargets);
      expect(listed, "the host's targets are read on the operator's session").to.not.be.undefined;
      expect(listed.sessionToken).to.equal(CURRENT_ACCESS_TOKEN);

      const [added] = b.callsTo(ScreenSharingService.method.addHostTarget);
      expect(added, "the probed endpoint is added on the operator's session").to.not.be.undefined;
      expect(added.sessionToken).to.equal(CURRENT_ACCESS_TOKEN);

      const [started] = b.callsTo(ScreenSharingService.method.startHostStream);
      expect(started, "the stream is started on the operator's session").to.not.be.undefined;
      expect(started.sessionToken).to.equal(CURRENT_ACCESS_TOKEN);
    });
  });
});

// ---------------------------------------------------------------------------
// AC-4 — closing the desktop releases the host's bridge
// ---------------------------------------------------------------------------

/**
 * A bridge is a process spawned on someone else's machine, so closing the overlay has to *say so*.
 *
 * The daemon side of AC-4 — that a `StopHostStream` releases the bridge — is proven in Rust. What
 * only a browser test can prove is that the browser ever asks: an overlay that unmounts silently
 * leaves one bridge process per desktop an operator ever looked at, and the daemon has nothing to
 * reconcile it against.
 */
describe("Closing a host desktop", () => {
  before(givenTheHostsPromptKey);

  beforeEach(() => {
    cy.clearLocalStorage();
  });

  it("asks the host to release the bridge the overlay started", () => {
    // Given a desktop the operator opened, with the start issued against a target on that host
    const feed = aHostPromptFeed();
    const backend = aDaemonAwaitingADesktopPassword(feed);
    mountWithMedia([aReachableVnc()], backend);
    hostDesktopPage.connect(HOST).click();
    cy.wrap(backend).should((b: InMemoryRpcBackend) =>
      expect(
        b.callsTo(ScreenSharingService.method.startHostStream),
        "there is nothing to release until a start has been issued",
      ).to.have.length(1),
    );

    // When they close it
    hostDesktopPage.close(HOST).click();

    // Then the browser asks that host to stop, naming the target the start actually used
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      const stops = b.callsTo(ScreenSharingService.method.stopHostStream);
      expect(stops, "closing the overlay must release the bridge").to.have.length(1);
      expect(stops[0].daemonInstanceId).to.equal(HOST);
      expect(stops[0].targetId).to.equal(A_HOST_TARGET);
    });

    // …and the desktop is gone from the screen, so the two cannot drift apart.
    hostDesktopPage.overlay(HOST).should("not.exist");
  });
});

// ---------------------------------------------------------------------------
// One feed, two kinds of question
// ---------------------------------------------------------------------------

/**
 * `StreamHostPrompts` is per **host**, not per surface, and `useHostPrompts` holds one slot.
 *
 * So every question that host raises — the desktop password this overlay asked for, and the key
 * passphrase `HostAddKeyAction` asks for on the same row — arrives in the same place and overwrites
 * whatever was there. Two distinct things have to hold, and each is pinned below: this surface must
 * never treat a key passphrase as its own question, and a key passphrase arriving must never take
 * the desktop's question away from the operator answering it.
 */

const A_QUESTION_ARRIVED = "a-question-arrived-probe";

/**
 * A bystander reading the same host's prompt feed, rendering whatever question arrives on it.
 *
 * It exists to make **delivery observable**. Raising a prompt is a frame handed to a generator, and
 * the browser has not necessarily rendered anything by the time the next assertion runs — so
 * "the desktop dialog did not open" asserted straight after a raise can pass simply because nothing
 * has happened yet. That is exactly how a missing filter reads as a working one. The probe consumes
 * the same feed as the overlay, so a question rendered here has been through the same commit as the
 * one the overlay decided not to ask about.
 */
function AQuestionArrivedProbe({ hostId }: { hostId: string }) {
  const question = useHostPrompts(hostId);
  return <span data-testid={A_QUESTION_ARRIVED}>{question?.subject ?? ""}</span>;
}

/** What the bystander has seen this host ask for. */
const aQuestionArrived = () => cy.get(`[data-testid="${A_QUESTION_ARRIVED}"]`);

/** Open the host's desktop with a bystander watching the same feed. */
function connectWithABystanderOn(feed: HostPromptFeed, backend: InMemoryRpcBackend) {
  mountWithRpc(
    withSelectedDaemon(
      <>
        <HostRowRemoteDesktop instanceId={HOST} readings={[aReachableVnc()]} />
        <AQuestionArrivedProbe hostId={HOST} />
      </>,
    ),
    backend,
  );
  hostDesktopPage.connect(HOST).click();
  cy.wrap(feed).should((f: HostPromptFeed) =>
    expect(
      f.subscriptionCount(),
      "the overlay and the bystander both read this host's questions",
    ).to.equal(2),
  );
}

describe("Host desktop password among the hosts other questions", () => {
  before(givenTheHostsPromptKey);

  beforeEach(() => {
    cy.clearLocalStorage();
  });

  it("asks the desktops own question and not a key passphrase on the same feed", () => {
    // Given an open host desktop whose daemon is blocked on the password
    const desktopFeed = aHostPromptFeed();
    connectWithABystanderOn(desktopFeed, aDaemonAwaitingADesktopPassword(desktopFeed));

    // When the host asks for that desktop's password
    cy.then(() => {
      desktopFeed.raise(theDesktopPasswordQuestion());
    });

    // Then the operator is asked. Stated first, so the absence below is this overlay declining a
    // question rather than an overlay that never asks anything at all.
    hostPassphraseDialogPage.root(HOST).should("contain.text", THE_DESKTOP);

    // When the same host instead raises the other question this feed carries — a key passphrase
    const keyFeed = aHostPromptFeed();
    connectWithABystanderOn(keyFeed, aDaemonAwaitingADesktopPassword(keyFeed));
    cy.then(() => {
      keyFeed.raise(aKeyPassphraseQuestion());
    });

    // …and it has reached this browser and been rendered
    aQuestionArrived().should("have.text", A_KEY_ON_THAT_HOST);

    // Then the operator is not asked it here. A desktop password typed into a key passphrase prompt
    // is sent as the answer to it: a secret handed to a question nobody on this surface asked.
    hostPassphraseDialogPage.root(HOST).should("not.exist");
  });

  it("keeps the desktop question answerable when another prompt lands mid answer", () => {
    // Given an operator part-way through answering the desktop's password question
    const feed = aHostPromptFeed();
    const backend = aDaemonAwaitingADesktopPassword(feed);
    connectWithABystanderOn(feed, backend);
    cy.then(() => {
      feed.raise(theDesktopPasswordQuestion());
    });
    hostPassphraseDialogPage.root(HOST).should("contain.text", THE_DESKTOP);
    hostPassphraseDialogPage.input().type(THE_DESKTOP_PASSWORD);

    // When an add-key passphrase question for the same host lands on the same feed
    cy.then(() => {
      feed.raise(aKeyPassphraseQuestion());
    });

    // …and has reached this browser and been rendered
    aQuestionArrived().should("have.text", A_KEY_ON_THAT_HOST);

    // Then the desktop's question is still in front of them, with what they typed still in it — a
    // dialog that unmounted here loses the typing and leaves the host's start blocked until the
    // prompt expires, with nothing left on screen to answer it.
    hostPassphraseDialogPage.root(HOST).should("contain.text", THE_DESKTOP);
    hostPassphraseDialogPage.input().should("have.value", THE_DESKTOP_PASSWORD);

    // …and it is still answerable, against the prompt the desktop is blocked on.
    hostPassphraseDialogPage.submit().click();
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      const answers = b.callsTo(HostService.method.answerHostPrompt);
      expect(answers, "the desktop question must still be answerable").to.have.length(1);
      expect(answers[0].promptId).to.equal(A_PROMPT);
    });
    theAnswerTheHostCanRead(backend).should("equal", THE_DESKTOP_PASSWORD);
  });
});

// ---------------------------------------------------------------------------
// Where the overlay is rendered
// ---------------------------------------------------------------------------

/**
 * The overlay is opened from inside the row, and must not be rendered there.
 *
 * `HostRowTooling` wraps the whole tooling section — including the remote-desktop block the connect
 * action lives in — in a `<span>`. A `<div>` is not permitted inside one: an HTML parser hoists it
 * out on any SSR or hydration path, so the tree the browser builds is not the tree React described,
 * and even client-side a block element dropped into the row's inline flex flow shifts the row for
 * as long as the desktop is open.
 *
 * This is the one test that mounts the section the way the screen mounts it. Mounted bare, as every
 * other test here mounts it, the overlay has no parent to be wrong about.
 */
describe("Host desktop overlay placement", () => {
  it("renders the overlay outside the rows inline flow", () => {
    // Given the row's tooling section, with a connectable desktop in it
    mountWithRpc(
      withSelectedDaemon(
        <HostRowTooling
          instanceId={HOST}
          git={undefined}
          githubCli={undefined}
          remoteDesktop={[aReachableVnc()]}
        />,
      ),
      anInMemoryRpcBackend(),
    );

    // When the operator opens the desktop
    hostDesktopPage.connect(HOST).click();

    // Then it is on screen
    hostDesktopPage.overlay(HOST).should("exist");

    // …and no inline element contains it.
    hostDesktopPage.inlineAncestorsOfOverlay(HOST).should("have.length", 0);
  });
});
