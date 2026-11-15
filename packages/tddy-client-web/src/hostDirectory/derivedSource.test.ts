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
import { DERIVED_SOURCE_ID, listDerivedHosts, type DerivedHostParent } from "./derivedSource";
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

describe("derived host directory source", () => {
  it("emits each connected daemon's derived hosts with their parent", async () => {
    // Given two connected daemons, each reaching hosts of its own
    const laptop = aDaemonReaching(LAPTOP, ["buildbox"]);
    const workstation = aDaemonReaching(WORKSTATION, ["gpu-rig", "staging"]);

    // When
    const source = await listDerivedHosts(
      [aParent(LAPTOP, laptop), aParent(WORKSTATION, workstation)],
      A_SESSION_TOKEN,
    );

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
    await listDerivedHosts([aParent(LAPTOP, laptop)], A_SESSION_TOKEN);

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
    const source = await listDerivedHosts(
      [aParent(LAPTOP, laptop), aParent(WORKSTATION, workstation, "error")],
      A_SESSION_TOKEN,
    );

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
    const source = await listDerivedHosts(
      [aParent(LAPTOP, laptop), aParent(WORKSTATION, workstation)],
      A_SESSION_TOKEN,
    );

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
    const source = await listDerivedHosts([aParent(LAPTOP, laptop)], A_SESSION_TOKEN);

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
