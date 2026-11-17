/**
 * Unit tests for the derived host-directory source.
 *
 * Each parent daemon answers from an in-memory `HostService` of its own, so a host appearing under
 * a parent is proof that parent was asked, and a parent's recorded calls are proof it was not.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-derived-hosts.md
 */

import { describe, it, expect } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import type { ConnectionStatus } from "../connections/types";
import {
  DerivedHostKind,
  DerivedHostSchema,
  HostService,
  ListDerivedHostsResponseSchema,
  ProbeOutcome,
  type DerivedHost,
} from "../gen/host_pb";
import { SessionTargetSchema, type SessionTarget } from "../gen/session_pb";
import { daemonHostOf, hostDescriptorOf } from "./daemonHost";
import {
  DERIVED_SOURCE_ID,
  NO_DERIVED_HOSTS,
  askParentForDerivedHosts,
  mergeParentAnswer,
  parentsToAsk,
  type DerivedHostDirectorySource,
  type DerivedHostParent,
} from "./derivedSource";
import type { HostDescriptor } from "./types";

const A_SESSION_TOKEN = "a-session-token";
const LAPTOP = "laptop-a";
const WORKSTATION = "workstation-b";

/** The target that starts a session on the SSH host `alias`. */
function theSshTarget(alias: string): SessionTarget {
  return create(SessionTargetSchema, { kind: { case: "ssh", value: { configHost: alias } } });
}

/** The SSH host `alias` as the daemon `parent` lists it. */
function anSshHostOf(parent: string, alias: string): DerivedHost {
  return create(DerivedHostSchema, {
    id: `${parent}/ssh/${alias}`,
    label: alias,
    kind: DerivedHostKind.SSH,
    parentInstanceId: parent,
    target: theSshTarget(alias),
  });
}

/** The directory entry the derived source contributes for the SSH host `alias` of `parent`. */
function aDerivedDescriptor(parent: string, alias: string): HostDescriptor {
  return {
    hostId: `${parent}/ssh/${alias}`,
    label: alias,
    sourceId: DERIVED_SOURCE_ID,
    parent,
    target: theSshTarget(alias),
  };
}

/** A daemon whose `HostService` lists `aliases` as the SSH hosts it reaches. */
function aDaemonReaching(parent: string, aliases: string[]): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(HostService, {
    listDerivedHosts: async () =>
      create(ListDerivedHostsResponseSchema, {
        outcome: ProbeOutcome.OK,
        hosts: aliases.map((alias) => anSshHostOf(parent, alias)),
      }),
  });
}

/** A daemon whose listing fails with `reason` — an SSH config it cannot read. */
function aDaemonThatCannotList(reason: string): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(HostService, {
    listDerivedHosts: async () =>
      create(ListDerivedHostsResponseSchema, {
        outcome: ProbeOutcome.FAILED,
        failureReason: reason,
      }),
  });
}

/** `hostId` as a parent the directory lists, answering from `backend`. */
function aParent(
  hostId: string,
  backend: InMemoryRpcBackend,
  status: ConnectionStatus = "connected",
): DerivedHostParent {
  return {
    host: { hostId, label: hostId, sourceId: "livekit" },
    status,
    client: createClient(HostService, backend.transport()),
  };
}

/** The source the app builds: ask each connected parent on its own, folding answers in as they arrive. */
async function aDerivedSourceFrom(
  parents: readonly DerivedHostParent[],
): Promise<DerivedHostDirectorySource> {
  let source: DerivedHostDirectorySource = NO_DERIVED_HOSTS;
  for (const parent of parentsToAsk(parents, new Set(), false)) {
    source = mergeParentAnswer(source, await askParentForDerivedHosts(parent, A_SESSION_TOKEN));
  }
  return source;
}

describe("derived host directory source", () => {
  it("emits each connected daemon's derived hosts with their parent", async () => {
    // Given two connected daemons, each reaching hosts of its own
    const laptop = aDaemonReaching(LAPTOP, ["buildbox"]);
    const workstation = aDaemonReaching(WORKSTATION, ["gpu-rig", "staging"]);

    // When
    const source = await aDerivedSourceFrom([aParent(LAPTOP, laptop), aParent(WORKSTATION, workstation)]);

    // Then every host is offered under the daemon that reaches it, in the daemons' order
    expect(source).toEqual({
      id: DERIVED_SOURCE_ID,
      status: "connected",
      error: null,
      hosts: [
        aDerivedDescriptor(LAPTOP, "buildbox"),
        aDerivedDescriptor(WORKSTATION, "gpu-rig"),
        aDerivedDescriptor(WORKSTATION, "staging"),
      ],
      failures: [],
    });
  });

  it("asks each daemon for its own hosts, as the signed-in session", async () => {
    // Given
    const laptop = aDaemonReaching(LAPTOP, ["buildbox"]);

    // When
    await aDerivedSourceFrom([aParent(LAPTOP, laptop)]);

    // Then the listing is addressed to that daemon, with the session's token
    expect(
      laptop
        .callsTo(HostService.method.listDerivedHosts)
        .map((call) => [call.daemonInstanceId, call.sessionToken]),
    ).toEqual([[LAPTOP, A_SESSION_TOKEN]]);
  });

  it("offers nothing for a parent that is offline", async () => {
    // Given a connected daemon, and one whose connection is down but would list a host if asked
    const laptop = aDaemonReaching(LAPTOP, ["buildbox"]);
    const workstation = aDaemonReaching(WORKSTATION, ["gpu-rig"]);

    // When
    const source = await aDerivedSourceFrom([
      aParent(LAPTOP, laptop),
      aParent(WORKSTATION, workstation, "error"),
    ]);

    // Then only the connected daemon's host is offered, and the offline one was never asked
    expect({
      hosts: source.hosts,
      askedOfTheOfflineDaemon: workstation.callsTo(HostService.method.listDerivedHosts).length,
    }).toEqual({
      hosts: [aDerivedDescriptor(LAPTOP, "buildbox")],
      askedOfTheOfflineDaemon: 0,
    });
  });

  it("keeps one parent's failure from hiding another parent's hosts", async () => {
    // Given a daemon that cannot read its SSH config, and one that lists a host
    const laptop = aDaemonThatCannotList("permission denied");
    const workstation = aDaemonReaching(WORKSTATION, ["gpu-rig"]);

    // When
    const source = await aDerivedSourceFrom([aParent(LAPTOP, laptop), aParent(WORKSTATION, workstation)]);

    // Then the other daemon's host is offered, and the failure is recorded against its own daemon
    expect(source).toEqual({
      id: DERIVED_SOURCE_ID,
      status: "connected",
      error: null,
      hosts: [aDerivedDescriptor(WORKSTATION, "gpu-rig")],
      failures: [{ parentHostId: LAPTOP, reason: "permission denied" }],
    });
  });

  it("is in error, with the reason, when every daemon it asked failed", async () => {
    // Given the only connected daemon cannot read its SSH config
    const laptop = aDaemonThatCannotList("permission denied");

    // When
    const source = await aDerivedSourceFrom([aParent(LAPTOP, laptop)]);

    // Then the source says so rather than reporting a daemon that reaches no hosts
    expect(source).toEqual({
      id: DERIVED_SOURCE_ID,
      status: "error",
      error: "permission denied",
      hosts: [],
      failures: [{ parentHostId: LAPTOP, reason: "permission denied" }],
    });
  });
});

describe("a derived host in the screens' vocabulary", () => {
  it("keeps its parent and its target as a DaemonHost", () => {
    // Given
    const descriptor = aDerivedDescriptor(LAPTOP, "buildbox");

    // When
    const host = daemonHostOf(descriptor);

    // Then
    expect(host).toEqual({
      instanceId: `${LAPTOP}/ssh/buildbox`,
      label: "buildbox",
      parent: LAPTOP,
      target: theSshTarget("buildbox"),
    });
  });

  it("keeps its parent and its target when described back to the directory", () => {
    // Given
    const descriptor = aDerivedDescriptor(LAPTOP, "buildbox");

    // When
    const described = hostDescriptorOf(daemonHostOf(descriptor), DERIVED_SOURCE_ID);

    // Then
    expect(described).toEqual(descriptor);
  });
});

// ---------------------------------------------------------------------------
// One parent at a time (`#remote-shell` 9/11)
// ---------------------------------------------------------------------------

/** A daemon whose `ListDerivedHosts` never answers — a listed peer nobody is home at. */
function aDaemonThatNeverAnswers(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(HostService, {
    listDerivedHosts: () => new Promise<never>(() => {}),
  });
}

const A_SHORT_DEADLINE_MS = 50;

describe("derived hosts, one parent at a time", () => {
  it("a parent that never answers withholds only its own hosts", async () => {
    // Given a daemon that lists buildbox and one that never answers
    const laptop = aDaemonReaching(LAPTOP, ["buildbox"]);
    const workstation = aDaemonThatNeverAnswers();

    // When each is asked on its own and the answers are folded in as they arrive
    const afterLaptop = mergeParentAnswer(
      NO_DERIVED_HOSTS,
      await askParentForDerivedHosts(aParent(LAPTOP, laptop), A_SESSION_TOKEN),
    );
    const afterBoth = mergeParentAnswer(
      afterLaptop,
      await askParentForDerivedHosts(aParent(WORKSTATION, workstation), A_SESSION_TOKEN, A_SHORT_DEADLINE_MS),
    );

    // Then buildbox is offered as soon as its daemon answered, and stays offered when the other times out
    expect({ early: afterLaptop.hosts, late: afterBoth.hosts, status: afterBoth.status }).toEqual({
      early: [aDerivedDescriptor(LAPTOP, "buildbox")],
      late: [aDerivedDescriptor(LAPTOP, "buildbox")],
      status: "connected",
    });
  });

  it("a call past its deadline is that parent's failure", async () => {
    // Given
    const workstation = aDaemonThatNeverAnswers();

    // When
    const answer = await askParentForDerivedHosts(
      aParent(WORKSTATION, workstation),
      A_SESSION_TOKEN,
      A_SHORT_DEADLINE_MS,
    );

    // Then the failure names the parent and the deadline, and no host is invented
    expect(answer).toEqual({
      parentHostId: WORKSTATION,
      hosts: [],
      failure: {
        parentHostId: WORKSTATION,
        reason: `${WORKSTATION} did not list its hosts within ${A_SHORT_DEADLINE_MS} ms`,
      },
    });
  });

  it("a parent's earlier answer is kept until a newer one arrives", async () => {
    // Given two daemons that have both answered once
    const laptopBefore = aDaemonReaching(LAPTOP, ["buildbox"]);
    const workstation = aDaemonReaching(WORKSTATION, ["gpu-rig"]);
    let source = mergeParentAnswer(
      NO_DERIVED_HOSTS,
      await askParentForDerivedHosts(aParent(LAPTOP, laptopBefore), A_SESSION_TOKEN),
    );
    source = mergeParentAnswer(
      source,
      await askParentForDerivedHosts(aParent(WORKSTATION, workstation), A_SESSION_TOKEN),
    );

    // When the laptop answers again with a host more
    const laptopAfter = aDaemonReaching(LAPTOP, ["buildbox", "staging"]);
    const updated = mergeParentAnswer(
      source,
      await askParentForDerivedHosts(aParent(LAPTOP, laptopAfter), A_SESSION_TOKEN),
    );

    // Then only the laptop's hosts changed, in their place; the workstation's stayed as they were
    expect(updated.hosts).toEqual([
      aDerivedDescriptor(LAPTOP, "buildbox"),
      aDerivedDescriptor(LAPTOP, "staging"),
      aDerivedDescriptor(WORKSTATION, "gpu-rig"),
    ]);
  });

  it("a failed listing replaces that parent's hosts with its failure and keeps the rest", async () => {
    // Given a daemon that answered, then can no longer read its config
    const laptop = aDaemonReaching(LAPTOP, ["buildbox"]);
    const workstation = aDaemonReaching(WORKSTATION, ["gpu-rig"]);
    let source = mergeParentAnswer(
      NO_DERIVED_HOSTS,
      await askParentForDerivedHosts(aParent(LAPTOP, laptop), A_SESSION_TOKEN),
    );
    source = mergeParentAnswer(
      source,
      await askParentForDerivedHosts(aParent(WORKSTATION, workstation), A_SESSION_TOKEN),
    );

    // When
    const failed = mergeParentAnswer(
      source,
      await askParentForDerivedHosts(aParent(LAPTOP, aDaemonThatCannotList("permission denied")), A_SESSION_TOKEN),
    );

    // Then
    expect({ hosts: failed.hosts, failures: failed.failures, status: failed.status }).toEqual({
      hosts: [aDerivedDescriptor(WORKSTATION, "gpu-rig")],
      failures: [{ parentHostId: LAPTOP, reason: "permission denied" }],
      status: "connected",
    });
  });

  it("a parent that becomes connected is asked, and a refresh asks every connected parent again", () => {
    // Given the laptop was asked while the workstation was still connecting
    const laptop = aParent(LAPTOP, aDaemonReaching(LAPTOP, []));
    const workstation = aParent(WORKSTATION, aDaemonReaching(WORKSTATION, []), "connecting");
    const asked = new Set([LAPTOP]);

    // When the workstation is still connecting, nothing is to ask; when it connects, it is; on a refresh, both are
    const whileConnecting = parentsToAsk([laptop, workstation], asked, false);
    const onceConnected = parentsToAsk([laptop, { ...workstation, status: "connected" }], asked, false);
    const onRefresh = parentsToAsk([laptop, { ...workstation, status: "connected" }], asked, true);

    // Then
    expect({
      whileConnecting: whileConnecting.map((p) => p.host.hostId),
      onceConnected: onceConnected.map((p) => p.host.hostId),
      onRefresh: onRefresh.map((p) => p.host.hostId),
    }).toEqual({ whileConnecting: [], onceConnected: [WORKSTATION], onRefresh: [LAPTOP, WORKSTATION] });
  });
});
