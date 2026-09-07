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
import {
  ConnectionService,
  HostPromptKind,
  HostRemoteDesktopSchema,
  ProbeOutcome,
} from "../../src/gen/connection_pb";
import { ScreenSharingService } from "../../src/gen/screen_sharing_pb";
import { HostRowRemoteDesktop } from "../../src/components/hosts/HostRowRemoteDesktop";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aHostConnection, aRegistryServing } from "../support/rpc/hostConnections";
import { aHostPromptFeed, type HostPromptFeed } from "../support/rpc/hostPromptFeed";
import { ConnectionProviders } from "../../src/rpc/connections/registry";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../src/hooks/authProvider";
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

/**
 * A real RSA-OAEP(SHA-256) public key in SPKI DER, as the daemon publishes with a prompt.
 *
 * Generated for real, exactly as `HostAddKeyAcceptance` does and for the same reason: a stubbed
 * `crypto.subtle` would hollow out the one assertion carrying this test's security claim.
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

/**
 * A daemon that takes the start and then stays blocked on the password — the state an operator is
 * actually looking at while the dialog is up.
 */
function aDaemonAwaitingADesktopPassword(feed: HostPromptFeed): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .implement(ConnectionService, {
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
    ...backend.callsTo(ConnectionService.method.answerHostPrompt),
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

describe("Host desktop password", () => {
  before(() => {
    cy.wrap(anRsaOaepPublicKey()).then((spkiDer) => {
      hostPublicKey = spkiDer as unknown as Uint8Array;
    });
  });

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
      feed.raise({
        promptId: A_PROMPT,
        daemonInstanceId: HOST,
        kind: HostPromptKind.DESKTOP_PASSWORD,
        subject: THE_DESKTOP,
        hostPublicKey,
        hostPublicKeyFingerprint: HOST_KEY_FINGERPRINT,
      });
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
      const answers = b.callsTo(ConnectionService.method.answerHostPrompt);
      expect(answers, "the answer must reach the host that raised the prompt").to.have.length(1);
      expect(answers[0].promptId).to.equal(A_PROMPT);
      expect(answers[0].daemonInstanceId).to.equal(HOST);
      expect(
        answers[0].encryptedAnswer.byteLength,
        "an RSA-OAEP ciphertext is key-sized; a plaintext password is not",
      ).to.be.greaterThan(64);
    });

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
    .implement(ConnectionService, {
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
    feed.raise({
      promptId: A_PROMPT,
      daemonInstanceId: HOST,
      kind: HostPromptKind.DESKTOP_PASSWORD,
      subject: THE_DESKTOP,
      hostPublicKey,
      hostPublicKeyFingerprint: HOST_KEY_FINGERPRINT,
    });
  });
}

describe("Host desktop without a password", () => {
  before(() => {
    cy.wrap(anRsaOaepPublicKey()).then((spkiDer) => {
      hostPublicKey = spkiDer as unknown as Uint8Array;
    });
  });

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
      const answers = b.callsTo(ConnectionService.method.answerHostPrompt);
      expect(answers, "the empty answer must reach the host that raised the prompt").to.have.length(
        1,
      );
      expect(answers[0].promptId).to.equal(A_PROMPT);
      expect(
        answers[0].encryptedAnswer.byteLength,
        "an RSA-OAEP ciphertext is key-sized whatever it encrypts",
      ).to.be.greaterThan(64);
    });

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
