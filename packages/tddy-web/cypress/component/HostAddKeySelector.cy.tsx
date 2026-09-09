/**
 * Acceptance tests: picking a key to load, instead of typing a path from memory.
 *
 * `AddHostKeyRequest.subject` is a path on the host, and until `ListHostKeyCandidates` existed there
 * was nothing to select *from* — the changeset promised a "key selector" and shipped a free-text
 * field. These specs pin what the selector is: the host says which of its operator's keys could be
 * loaded, and the operator picks one.
 *
 * Two properties carry the security of this surface, and both are asserted here rather than assumed
 * from the daemon's tests:
 *
 * - **What is offered is what the add accepts.** A picked key goes straight back as `subject`, so a
 *   path this list shows and the add refuses is a choice that does not work. The daemon's
 *   `offers_only_paths_that_an_add_of_the_same_key_accepts` states this on its side; here it is
 *   stated as "the request carries the path that was picked, unchanged".
 * - **A list of keys is not a use of them.** The listing describes each candidate from its public
 *   half, so nothing in this UI can display key material — there is none in the message.
 *
 * Feature: docs/ft/web/hosts-screen-add-key.md
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { Code } from "@connectrpc/connect";
import {
  AddHostKeyOutcome,
  ConnectionService,
  HostSshAgentSchema,
  ProbeOutcome,
  type HostSshAgent,
} from "../../src/gen/connection_pb";
import { HostAddKeyAction } from "../../src/components/hosts/HostAddKeyAction";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aHostPromptFeed } from "../support/rpc/hostPromptFeed";
import { hostAddKeyPage as addKey, hostAddKeyOutcome } from "../support/pages/hostsScreenPage";

const HOST = "workstation-1";

/** Two keys of this host's operator, as `ListHostKeyCandidates` describes them. */
const AN_ED25519_KEY = {
  path: "/home/ada/.ssh/id_ed25519",
  keyType: "ssh-ed25519",
  fingerprint: "SHA256:ZLBiCcwTvIcQUyRnvSHhpsdgWLLLZtWbBAPtgWNBpAg",
};
const AN_RSA_KEY = {
  path: "/home/ada/.ssh/id_rsa",
  keyType: "ssh-rsa",
  fingerprint: "SHA256:9WK1EJ1YHXbCP9V0Y13uwbHFuqWFcAe1eFf0kSPn5Ok",
};

/** A key of theirs that no listing can see — no `.pub` beside it — so it can only be typed. */
const AN_UNLISTED_KEY_PATH = "/home/ada/keys/deploy_key";

/** An ssh-agent that answered for this host and is holding nothing — the state that wants a key. */
function anEmptyAgent(): HostSshAgent {
  return create(HostSshAgentSchema, { outcome: ProbeOutcome.OK, reachable: true, keys: [] });
}

/** A host whose agent did not answer, so there is nothing to load a key into. */
function noAgent(): HostSshAgent {
  return create(HostSshAgentSchema, { outcome: ProbeOutcome.OK, reachable: false, keys: [] });
}

/**
 * A daemon that offers `candidates` and accepts whatever is added.
 *
 * `addHostKey` answers at once with `ADDED`: these specs are about which path leaves the browser,
 * and a prompt round trip in the middle of that would only obscure it. The prompt feed is still
 * wired, because the component subscribes to it for the duration of the call.
 */
function aBackendOffering(
  candidates: Array<{ path: string; keyType: string; fingerprint: string }>,
): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(ConnectionService, {
    ...aHostPromptFeed().handlers,
    listHostKeyCandidates: async () => ({ candidates }),
    addHostKey: async () => ({
      added: true,
      outcome: AddHostKeyOutcome.ADDED,
      fingerprint: "SHA256:whatever-the-agent-reported",
      failureReason: "",
    }),
  });
}

function mountAction(backend: InMemoryRpcBackend, sshAgent: HostSshAgent = anEmptyAgent()) {
  mountWithRpc(
    withSelectedDaemon(<HostAddKeyAction instanceId={HOST} sshAgent={sshAgent} />, [
      { instanceId: HOST, label: HOST },
    ]),
    backend,
  );
}

describe("The keys a host offers to load", () => {
  it("offers the keys the host reported, so nothing has to be typed from memory", () => {
    // Given a host whose operator has two keys in their ~/.ssh
    mountAction(aBackendOffering([AN_ED25519_KEY, AN_RSA_KEY]));

    // When the operator looks at the row
    // Then both keys are on offer, each named by its path
    addKey.keyChoices(HOST).should("contain.text", AN_ED25519_KEY.path);
    addKey.keyChoices(HOST).should("contain.text", AN_RSA_KEY.path);
  });

  it("tells the keys apart by type and fingerprint, not by path alone", () => {
    // Given two keys whose file names say nothing about which is which
    mountAction(aBackendOffering([AN_ED25519_KEY, AN_RSA_KEY]));

    // When the operator reads the choices
    // Then each carries what the daemon derived from its public half — the fingerprint is what an
    // operator matches against the agent's own key list, and against the host out of band
    addKey.keyChoices(HOST).should("contain.text", AN_ED25519_KEY.keyType);
    addKey.keyChoices(HOST).should("contain.text", AN_ED25519_KEY.fingerprint);
    addKey.keyChoices(HOST).should("contain.text", AN_RSA_KEY.keyType);
    addKey.keyChoices(HOST).should("contain.text", AN_RSA_KEY.fingerprint);
  });

  it("asks the host in the row, and only when there is an agent to add to", () => {
    // Given a host whose agent did not answer
    const unreachable = aBackendOffering([AN_ED25519_KEY]);
    mountAction(unreachable, noAgent());

    // Then nothing was asked: a row that cannot take a key must not enumerate one's keys either
    cy.wrap(unreachable).should((b: InMemoryRpcBackend) => {
      expect(
        b.callsTo(ConnectionService.method.listHostKeyCandidates),
        "a host with no agent was asked for its keys",
      ).to.have.length(0);
    });

    // Given the same host with an agent that answered
    const reachable = aBackendOffering([AN_ED25519_KEY]);
    mountAction(reachable);

    // Then the listing names the host in this row. The keys are files on one machine, so a listing
    // addressed to nobody in particular would offer paths from whichever daemon took the call.
    cy.wrap(reachable).should((b: InMemoryRpcBackend) => {
      const calls = b.callsTo(ConnectionService.method.listHostKeyCandidates);
      expect(calls, "the row asks its own host what keys it has").to.have.length(1);
      expect(calls[0].daemonInstanceId).to.equal(HOST);
    });
  });

  it("adds the key that was picked, at the path the host gave for it", () => {
    // Given a host offering two keys
    const backend = aBackendOffering([AN_ED25519_KEY, AN_RSA_KEY]);
    mountAction(backend);

    // When the operator picks the second one and starts the add
    addKey.pickKey(HOST, AN_RSA_KEY.path);
    addKey.start(HOST).click();

    // Then the path that leaves is the one the host offered, unchanged. The daemon reads `subject`
    // under a confinement this browser cannot see, so a path this UI adorned or abbreviated would
    // be refused with a message that names no path.
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      const calls = b.callsTo(ConnectionService.method.addHostKey);
      expect(calls, "exactly one add is sent for one picked key").to.have.length(1);
      expect(calls[0].subject).to.equal(AN_RSA_KEY.path);
      expect(calls[0].daemonInstanceId).to.equal(HOST);
    });
  });

  it("still takes a typed path for a key no listing can see", () => {
    // Given a host that offers nothing — every key its operator has was made without keeping a
    // `.pub`, which is the one thing that makes a key invisible to the listing
    const backend = aBackendOffering([]);
    mountAction(backend);

    // When the operator names one of them themselves
    addKey.addKey(HOST, AN_UNLISTED_KEY_PATH);

    // Then it is added like any other. The list is a convenience over ~/.ssh, not the boundary of
    // what may be loaded, so removing the field would make those keys unloadable from here.
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      const calls = b.callsTo(ConnectionService.method.addHostKey);
      expect(calls, "a typed path is still an add").to.have.length(1);
      expect(calls[0].subject).to.equal(AN_UNLISTED_KEY_PATH);
    });
  });

  it("treats a host that will not list its keys as one with no keys to offer", () => {
    // Given a host whose daemon is too old to know this call, or refused it
    const backend = anInMemoryRpcBackend()
      .implement(ConnectionService, {
        ...aHostPromptFeed().handlers,
        addHostKey: async () => ({
          added: true,
          outcome: AddHostKeyOutcome.ADDED,
          fingerprint: "SHA256:whatever-the-agent-reported",
          failureReason: "",
        }),
      })
      .failWith(ConnectionService.method.listHostKeyCandidates, Code.Unimplemented);
    mountAction(backend);

    // When the operator names a key themselves
    addKey.addKey(HOST, AN_UNLISTED_KEY_PATH);

    // Then the add works and nothing on the row reports the failed listing. A host that cannot say
    // what keys it has is not a host that cannot take one, and an error here would put a
    // never-before-seen call in front of an operator whose next action is unaffected by it.
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      expect(b.callsTo(ConnectionService.method.addHostKey)).to.have.length(1);
    });
    hostAddKeyOutcome.notSaying(HOST, /list|candidate|unimplemented/i);
  });
});
