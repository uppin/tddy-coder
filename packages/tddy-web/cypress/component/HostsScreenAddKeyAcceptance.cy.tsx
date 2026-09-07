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
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-06-agent-add-key.md
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
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aHostPromptFeed, type HostPromptFeed } from "../support/rpc/hostPromptFeed";
import {
  hostAddKeyPage as addKey,
  hostPassphraseDialogPage as dialog,
} from "../support/pages/hostsScreenPage";

const HOST = "workstation-1";
const KEY_PATH = "/home/ada/.ssh/id_ed25519";
const ADDED_FINGERPRINT = "SHA256:JfISx02kSjWJevGy/MjUdXCv76HaRM3YkYNvepTyHD8";
const HOST_KEY_FINGERPRINT = "SHA256:ZLBiCcwTvIcQUyRnvSHhpsdgWLLLZtWbBAPtgWNBpAg";

/**
 * Stand-in for the host's published SPKI DER key.
 *
 * Not real key material, and deliberately so: no test in this file submits an answer, so nothing
 * here imports it. The one test that genuinely encrypts lives in `HostAddKeyAcceptance.cy.tsx` and
 * generates a real RSA-OAEP key for exactly that reason.
 */
const A_PUBLISHED_HOST_KEY = new Uint8Array([0x30, 0x82, 0x01, 0x22]);

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

// ---------------------------------------------------------------------------

describe("Hosts screen add key", () => {
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
    addKey.addKey(HOST, KEY_PATH);
    cy.wrap(feed).should((f: HostPromptFeed) => expect(f.subscriptionCount()).to.equal(1));

    // When the host raises its passphrase question
    cy.then(() => {
      feed.raise({
        promptId: "prompt-1",
        daemonInstanceId: HOST,
        subject: KEY_PATH,
        hostPublicKey: A_PUBLISHED_HOST_KEY,
        hostPublicKeyFingerprint: HOST_KEY_FINGERPRINT,
      });
    });

    // Then the operator sees the dialog, naming the host, the key and the fingerprint to verify
    dialog.root(HOST).should("contain.text", KEY_PATH).and("contain.text", HOST_KEY_FINGERPRINT);
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
