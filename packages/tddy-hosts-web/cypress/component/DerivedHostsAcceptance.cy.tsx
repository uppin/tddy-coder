/**
 * Acceptance tests: the Hosts screen lists the hosts a daemon reaches through a remote shell —
 * its SSH config aliases — beneath that daemon's own row, marked with their kind, and without the
 * cells only a daemon can fill.
 *
 * A derived host has no daemon: nothing probes it, so a telemetry or tooling cell on its row would
 * be a reading nobody took. And a daemon that could not list is not a daemon that reaches nothing:
 * the failure is shown on the parent's row, in the daemon's own words.
 *
 * The backend implements `host.HostService` only: the known-hosts listing and `ListDerivedHosts`.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-derived-hosts.md
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  DerivedHostKind,
  DerivedHostSchema,
  HostService,
  ListDerivedHostsResponseSchema,
  ProbeOutcome,
  type KnownHostEntry,
  type ListDerivedHostsResponse,
} from "tddy-client-web/gen/host_pb";
import { HostsContent } from "../../src/HostsContent";
import { derivedHostRowTestId } from "../../src/DerivedHostRows";
import { derivedHostsPage, hostsScreenPage } from "../support/pages/hostsScreenPage";

const PARENT = "workstation-1";
const BUILDBOX = `${PARENT}/ssh/buildbox`;
const GPU_RIG = `${PARENT}/ssh/gpu-rig`;
/** The directory lists the daemon its rows are about, so the screen has a connection to ask over. */
const PARENT_LISTED = [{ instanceId: PARENT, label: PARENT }];

/** The daemon the derived hosts hang off: online, and the one serving the page. */
function theParentDaemon(): KnownHostEntry {
  return {
    instanceId: PARENT,
    label: `${PARENT} (this daemon)`,
    online: true,
    firstSeenUnixMs: BigInt(Date.now() - 86_400_000),
    lastSeenUnixMs: BigInt(Date.now()),
    reposBasePath: "repos",
    maxAttachmentBytes: 0n,
    isLocal: true,
  } as KnownHostEntry;
}

/** A listing of `aliases` as the SSH hosts the parent reaches. */
function aListingOfSshHosts(aliases: string[]): ListDerivedHostsResponse {
  return create(ListDerivedHostsResponseSchema, {
    outcome: ProbeOutcome.OK,
    hosts: aliases.map((alias) =>
      create(DerivedHostSchema, {
        id: `${PARENT}/ssh/${alias}`,
        label: alias,
        kind: DerivedHostKind.SSH,
        parentInstanceId: PARENT,
        target: { kind: { case: "ssh", value: { configHost: alias } } },
      }),
    ),
  });
}

/** A listing that could not be made, for `reason`. */
function aFailedListing(reason: string): ListDerivedHostsResponse {
  return create(ListDerivedHostsResponseSchema, {
    outcome: ProbeOutcome.FAILED,
    failureReason: reason,
  });
}

/** A daemon that knows itself and answers `ListDerivedHosts` with `listing`. */
function aDaemonAnswering(listing: ListDerivedHostsResponse): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(HostService, {
    listKnownHosts: () => ({ hosts: [theParentDaemon()] }),
    listDerivedHosts: async () => listing,
  });
}

function mountHosts(backend: InMemoryRpcBackend) {
  cy.mountWithClients(<HostsContent />, backend, { daemons: PARENT_LISTED });
}

describe("Hosts screen derived hosts", () => {
  it("lists derived hosts beneath their parent, marked SSH", () => {
    // Given a daemon that reaches buildbox and gpu-rig over SSH
    const backend = aDaemonAnswering(aListingOfSshHosts(["buildbox", "gpu-rig"]));

    // When
    mountHosts(backend);

    // Then both are rows of their own directly beneath the daemon's, in the listing's order
    derivedHostsPage
      .tableRowTestIds()
      .should("deep.equal", [
        `hosts-row-${PARENT}`,
        derivedHostRowTestId(BUILDBOX),
        derivedHostRowTestId(GPU_RIG),
      ]);
    // And each says what it is, and what it is reached through
    derivedHostsPage.row(BUILDBOX).should("contain.text", "buildbox");
    derivedHostsPage.kind(BUILDBOX).should("have.text", "SSH");
    derivedHostsPage.row(GPU_RIG).should("contain.text", "gpu-rig");
    derivedHostsPage.kind(GPU_RIG).should("have.text", "SSH");
  });

  it("shows no telemetry or tooling cell on a derived host", () => {
    // Given a daemon that reaches buildbox
    const backend = aDaemonAnswering(aListingOfSshHosts(["buildbox"]));

    // When
    mountHosts(backend);

    // Then the row is there — without it the cells would be absent merely because nothing rendered
    derivedHostsPage.row(BUILDBOX).should("exist");
    // And it carries none of the cells only a daemon can fill
    derivedHostsPage.daemonOnlyCells(BUILDBOX).should("have.length", 0);
  });

  it("shows a listing failure on the parent's row", () => {
    // Given a daemon whose SSH config cannot be read
    const backend = aDaemonAnswering(aFailedListing("permission denied"));

    // When
    mountHosts(backend);

    // Then the daemon's row says the listing failed, and why
    hostsScreenPage.row(PARENT).should("exist");
    derivedHostsPage.listingFailure(PARENT).should("contain.text", "permission denied");
    // And the failure is not dressed as a daemon that reaches no hosts
    derivedHostsPage.emptyState(PARENT).should("not.exist");
  });
});

// ---------------------------------------------------------------------------
// `#remote-shell` 9/11 — a row whose daemon answers on another id than it is filed under
// ---------------------------------------------------------------------------

/** The id the daemon answers RPCs on while this run lasts: its durable id plus its startup stamp. */
const PARENT_ROUTING = `${PARENT}-1794836073070`;

/** The parent as a daemon that appends its startup timestamp lists itself: filed under `PARENT`, answering on `PARENT_ROUTING`. */
function theParentDaemonAnsweringOn(routingInstanceId: string): KnownHostEntry {
  return { ...theParentDaemon(), routingInstanceId } as KnownHostEntry;
}

describe("Hosts screen derived hosts of a daemon with a routing id", () => {
  it("asks an online row's derived hosts through its routing id", () => {
    // Given a daemon filed under its durable id that answers on its routing id, reaching buildbox
    const backend = anInMemoryRpcBackend().implement(HostService, {
      listKnownHosts: () => ({ hosts: [theParentDaemonAnsweringOn(PARENT_ROUTING)] }),
      listDerivedHosts: async () => aListingOfSshHosts(["buildbox"]),
    });
    // And the directory lists that daemon by the id it answers on — the only id a connection exists under
    cy.mountWithClients(<HostsContent />, backend, {
      daemons: [{ instanceId: PARENT_ROUTING, label: PARENT_ROUTING }],
    });

    // Then the row is asked, and buildbox is listed beneath it
    derivedHostsPage.row(BUILDBOX).should("exist");
    cy.wrap(null).should(() => {
      expect(
        backend.callsTo(HostService.method.listDerivedHosts).map((call) => call.daemonInstanceId),
      ).to.deep.equal([PARENT_ROUTING]);
    });
  });
});
