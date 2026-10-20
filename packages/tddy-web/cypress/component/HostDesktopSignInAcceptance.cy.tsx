/**
 * Acceptance tests: a host desktop's sign-in is shaped by the methods the desktop advertises.
 *
 * Opening a desktop first asks its host which sign-in methods it advertises (`InspectHostDesktop`).
 * A desktop that wants none opens without a form; one that wants something is answered through a
 * `DESKTOP_CREDENTIALS` question whose form asks for exactly what the chosen method needs, with the
 * strongest method preselected and a Remember box. A desktop that refuses the sign-in is asked
 * about again, with its reason; one that advertises nothing tddy supports says so.
 *
 * What goes back on the wire is asserted by **decrypting** it with the private half of the key the
 * question published (`anRsaOaepKeypair`) and decoding the `DesktopCredentialAnswer` inside: every
 * RSA-2048 ciphertext is 256 bytes whatever it carries, so only the plaintext says what was sent.
 *
 * Feature: docs/ft/web/screen-sharing-sessions.md (AC-SS-15, AC-SS-19)
 */

import { create, fromBinary } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  HostPromptKind,
  HostRemoteDesktopSchema,
  HostService,
  ProbeOutcome,
} from "../../src/gen/host_pb";
import {
  DesktopAuthMethod,
  DesktopCredentialAnswerSchema,
  ScreenSharingService,
  type DesktopCredentialAnswer,
} from "../../src/gen/screen_sharing_pb";
import { HostRowRemoteDesktop } from "../../src/components/hosts/HostRowRemoteDesktop";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import {
  aHostPromptFeed,
  type HostPromptFeed,
  type HostPromptFrame,
} from "../support/rpc/hostPromptFeed";
import { anRsaOaepKeypair, type AHostPromptKeypair } from "../support/hostKeys";
import { hostDesktopPage, hostDesktopSignInPage } from "../support/pages/hostsScreenPage";

const HOST = "workstation-1";
/** The directory lists the host its row is about — a row exists only for a listed host. */
const HOST_LISTED = [{ instanceId: HOST, label: HOST }];
const VNC = 1;
const A_HOST_TARGET = "target-host-desktop-1";
const THE_DESKTOP = "Desktop (127.0.0.1:5900)";
const HOST_KEY_FINGERPRINT = "SHA256:ZLBiCcwTvIcQUyRnvSHhpsdgWLLLZtWbBAPtgWNBpAg";

const THE_FIRST_QUESTION = "prompt-desktop-credentials-1";
const THE_SECOND_QUESTION = "prompt-desktop-credentials-2";

const THE_ACCOUNT = "ada";
const THE_PASSWORD = "correct horse battery staple";
/** What the desktop said when it turned the sign-in down, as the daemon passes it on. */
const THE_DESKTOPS_REASON = "the password is not right for this desktop";

/** VNC security types tddy cannot sign in with: Tight (16) and UltraVNC MS-Logon (22). */
const TIGHT = 16;
const ULTRA_MS_LOGON = 22;

/** The host's prompt keypair. The private half is what makes an answer checkable. */
let hostKey: AHostPromptKeypair;

function givenTheHostsPromptKey() {
  cy.wrap(anRsaOaepKeypair()).then((keypair) => {
    hostKey = keypair as unknown as AHostPromptKeypair;
  });
}

function aReachableVnc() {
  return create(HostRemoteDesktopSchema, {
    outcome: ProbeOutcome.OK,
    protocol: VNC,
    canBridge: true,
    desktopReachable: true,
    port: 5900,
  });
}

// ── The host ──────────────────────────────────────────────────────────────────

/** What the host's desktop advertises, as its bridge read it. */
interface TheAdvertisement {
  offered: DesktopAuthMethod[];
  unsupportedVncTypes?: number[];
}

/** How the host's `StartHostStream` behaves. */
type TheStart =
  /** Opens at once: nothing to ask. */
  | "opens"
  /** Blocks on the question it raised, for as long as the test runs. */
  | "waits for the answer"
  /** The first start refuses once the answer arrived; every later one blocks on its question. */
  | "refuses the first sign in";

const A_STREAM = {
  livekitRoom: `host-desktop-${HOST}`,
  livekitUrl: "wss://livekit.example.test",
  bridgeIdentity: "bridge-workstation-1",
  trackName: "desktop",
  width: 1280,
  height: 800,
};

/** A host whose desktop advertises `advertisement`, and whose start does what `start` says. */
function aHostWhoseDesktop(
  advertisement: TheAdvertisement,
  start: TheStart,
  feed: HostPromptFeed,
): InMemoryRpcBackend {
  let theAnswerArrived: () => void = () => undefined;
  const answered = new Promise<void>((resolve) => {
    theAnswerArrived = resolve;
  });
  let starts = 0;
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
      inspectHostDesktop: async () => ({
        offered: advertisement.offered,
        unsupportedVncTypes: advertisement.unsupportedVncTypes ?? [],
        remembered: false,
      }),
      startHostStream: async () => {
        starts += 1;
        if (start === "opens") return A_STREAM;
        if (start === "refuses the first sign in" && starts === 1) {
          await answered;
          throw new ConnectError(THE_DESKTOPS_REASON, Code.Unauthenticated);
        }
        return new Promise(() => undefined);
      },
      stopHostStream: async () => ({ ok: true }),
    });
}

/** The host's `DESKTOP_CREDENTIALS` question, as it raises it. */
function theCredentialsQuestion(
  promptId: string,
  offered: DesktopAuthMethod[],
): HostPromptFrame {
  return {
    promptId,
    daemonInstanceId: HOST,
    kind: HostPromptKind.DESKTOP_CREDENTIALS,
    subject: THE_DESKTOP,
    hostPublicKey: hostKey.spkiDer,
    hostPublicKeyFingerprint: HOST_KEY_FINGERPRINT,
    offeredAuthMethods: offered,
  };
}

// ── The operator ──────────────────────────────────────────────────────────────

/** Open the host's desktop from its row. */
function openTheDesktopOn(backend: InMemoryRpcBackend) {
  mountWithRpc(
    withSelectedDaemon(
      <HostRowRemoteDesktop instanceId={HOST} readings={[aReachableVnc()]} />,
      HOST_LISTED,
    ),
    backend,
  );
  hostDesktopPage.connect(HOST).click();
}

/** Open the host's desktop and arrive at the moment the host has asked for its credentials. */
function openAndBeAskedFor(
  advertisement: TheAdvertisement,
  start: TheStart = "waits for the answer",
): { backend: InMemoryRpcBackend; feed: HostPromptFeed } {
  const feed = aHostPromptFeed();
  const backend = aHostWhoseDesktop(advertisement, start, feed);
  openTheDesktopOn(backend);
  // …with the browser listening to that host's questions, or the host is asking nobody
  cy.wrap(feed).should((f: HostPromptFeed) => expect(f.subscriptionCount()).to.equal(1));
  cy.then(() => {
    feed.raise(theCredentialsQuestion(THE_FIRST_QUESTION, advertisement.offered));
  });
  hostDesktopSignInPage.root(HOST).should("be.visible");
  return { backend, feed };
}

/**
 * The credential the host reads out of the single answer the browser sent it.
 *
 * Decrypted with the private half of the key the question published, then decoded: the claim is
 * what the operator sent, not that something was sent.
 */
function theCredentialTheHostCanRead(
  backend: InMemoryRpcBackend,
): Cypress.Chainable<DesktopCredentialAnswer> {
  return cy
    .wrap(backend)
    .should((b: InMemoryRpcBackend) =>
      expect(
        b.callsTo(HostService.method.answerHostPrompt),
        "exactly one answer must reach the host that raised the question",
      ).to.have.length(1),
    )
    .then((b: InMemoryRpcBackend) =>
      hostKey.decryptBytes(b.callsTo(HostService.method.answerHostPrompt)[0].encryptedAnswer),
    )
    .then((plaintext) => fromBinary(DesktopCredentialAnswerSchema, plaintext as Uint8Array));
}

/** The parts of an answer a spec states, as plain values. */
function whatItSays(answer: DesktopCredentialAnswer) {
  return {
    method: answer.method,
    username: answer.username,
    domain: answer.domain,
    password: answer.password,
    remember: answer.remember,
  };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("Host desktop sign-in", () => {
  before(givenTheHostsPromptKey);

  beforeEach(() => {
    cy.viewport(1200, 800);
    cy.clearLocalStorage();
  });

  it("opens a desktop that needs no sign in without asking", () => {
    // Given a desktop that advertises no authentication
    const feed = aHostPromptFeed();
    const backend = aHostWhoseDesktop({ offered: [DesktopAuthMethod.NONE] }, "opens", feed);

    // When the operator opens it
    openTheDesktopOn(backend);

    // Then it opens
    hostDesktopPage.stream().should("exist");
    // …having asked the host what it advertises, for the target being opened
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      const inspections = b.callsTo(ScreenSharingService.method.inspectHostDesktop);
      expect(inspections, "the desktop is inspected before it is opened").to.have.length(1);
      expect(inspections[0].targetId).to.equal(A_HOST_TARGET);
    });
    // …and without a form, or an answer sent to anyone
    hostDesktopSignInPage.root(HOST).should("not.exist");
    cy.wrap(backend).should((b: InMemoryRpcBackend) =>
      expect(b.callsTo(HostService.method.answerHostPrompt)).to.have.length(0),
    );
  });

  it("asks only for a password for vnc authentication", () => {
    // When the host asks for the credentials of a VNC Authentication desktop
    openAndBeAskedFor({ offered: [DesktopAuthMethod.VNC_PASSWORD] });

    // Then the form asks for a password and nothing else
    hostDesktopSignInPage.password().should("be.visible");
    hostDesktopSignInPage.username().should("not.exist");
    hostDesktopSignInPage.domain().should("not.exist");
  });

  it("asks for a username and password for apple remote desktop", () => {
    // When the host asks for the credentials of an Apple Remote Desktop
    openAndBeAskedFor({ offered: [DesktopAuthMethod.APPLE_REMOTE_DESKTOP] });

    // Then the form asks for a username and a password, and no domain
    hostDesktopSignInPage.username().should("be.visible");
    hostDesktopSignInPage.password().should("be.visible");
    hostDesktopSignInPage.domain().should("not.exist");
  });

  it("asks for a username domain and password for nla", () => {
    // When the host asks for the credentials of an RDP desktop that requires NLA
    openAndBeAskedFor({ offered: [DesktopAuthMethod.RDP_NLA] });

    // Then the form asks for a username, a domain and a password
    hostDesktopSignInPage.username().should("be.visible");
    hostDesktopSignInPage.domain().should("be.visible");
    hostDesktopSignInPage.password().should("be.visible");
  });

  it("preselects the strongest method and lets the operator switch", () => {
    // Given a desktop advertising both VNC Authentication and Apple Remote Desktop
    const { backend } = openAndBeAskedFor({
      offered: [DesktopAuthMethod.VNC_PASSWORD, DesktopAuthMethod.APPLE_REMOTE_DESKTOP],
    });

    // Then Apple Remote Desktop — the stronger of the two — is the one being asked for
    hostDesktopSignInPage
      .method()
      .should("have.value", String(DesktopAuthMethod.APPLE_REMOTE_DESKTOP));
    hostDesktopSignInPage.username().should("be.visible");

    // When the operator switches to VNC Authentication and signs in with a password
    hostDesktopSignInPage.chooseMethod(DesktopAuthMethod.VNC_PASSWORD);
    hostDesktopSignInPage.username().should("not.exist");
    hostDesktopSignInPage.password().type(THE_PASSWORD);
    hostDesktopSignInPage.submit().click();

    // Then the host reads a VNC Authentication credential with that password
    theCredentialTheHostCanRead(backend)
      .then(whatItSays)
      .should("deep.equal", {
        method: DesktopAuthMethod.VNC_PASSWORD,
        username: "",
        domain: "",
        password: THE_PASSWORD,
        remember: false,
      });
  });

  it("sends remember with the credential when it is checked", () => {
    // Given the host asking for an Apple Remote Desktop's credentials
    const { backend } = openAndBeAskedFor({
      offered: [DesktopAuthMethod.APPLE_REMOTE_DESKTOP],
    });

    // When the operator signs in with Remember ticked
    hostDesktopSignInPage.username().type(THE_ACCOUNT);
    hostDesktopSignInPage.password().type(THE_PASSWORD);
    hostDesktopSignInPage.remember().check();
    hostDesktopSignInPage.submit().click();

    // Then the host reads the credential, asking to be remembered
    theCredentialTheHostCanRead(backend)
      .then(whatItSays)
      .should("deep.equal", {
        method: DesktopAuthMethod.APPLE_REMOTE_DESKTOP,
        username: THE_ACCOUNT,
        domain: "",
        password: THE_PASSWORD,
        remember: true,
      });
  });

  it("asks again with the desktops reason after a refused sign in", () => {
    // Given a desktop that refuses the first sign-in
    const offered = [DesktopAuthMethod.VNC_PASSWORD];
    const { backend, feed } = openAndBeAskedFor({ offered }, "refuses the first sign in");
    hostDesktopSignInPage.password().type("not the password");
    hostDesktopSignInPage.submit().click();

    // When the viewer starts the desktop again and the host asks again
    cy.wrap(backend).should((b: InMemoryRpcBackend) =>
      expect(
        b.callsTo(ScreenSharingService.method.startHostStream),
        "a refused sign-in is followed by another start, so the host can ask again",
      ).to.have.length(2),
    );
    cy.then(() => {
      feed.raise(theCredentialsQuestion(THE_SECOND_QUESTION, offered));
    });

    // Then the form is in front of the operator again, saying why the desktop refused
    hostDesktopSignInPage.root(HOST).should("be.visible");
    hostDesktopSignInPage.error().should("contain.text", THE_DESKTOPS_REASON);
  });

  it("says the desktop offers no sign in tddy supports", () => {
    // Given a desktop that advertises only sign-in methods tddy cannot complete
    const feed = aHostPromptFeed();
    const backend = aHostWhoseDesktop(
      { offered: [], unsupportedVncTypes: [TIGHT, ULTRA_MS_LOGON] },
      "waits for the answer",
      feed,
    );

    // When the operator opens it
    openTheDesktopOn(backend);

    // Then the viewer says so, naming what the desktop did advertise
    hostDesktopPage
      .status(HOST)
      .invoke("text")
      .should("match", /sign.in/i)
      .and("contain", String(TIGHT))
      .and("contain", String(ULTRA_MS_LOGON));
    // …and offers no form, and starts nothing
    hostDesktopSignInPage.root(HOST).should("not.exist");
    cy.wrap(backend).should((b: InMemoryRpcBackend) =>
      expect(b.callsTo(ScreenSharingService.method.startHostStream)).to.have.length(0),
    );
  });
});
