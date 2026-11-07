/**
 * Acceptance tests: the Hosts screen (`#/hosts`) lists every host tddy has a record of — including
 * hosts that are not reachable right now — with an online/offline state and a last-seen time.
 *
 * The behaviour that matters most here is the **offline row**. Every other surface in tddy derives
 * its host list from the live common-room roster, so a host that leaves simply vanishes. This screen
 * exists so it does not.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-06-host-registry.md
 */

import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { HostsContent } from "../../src/HostsContent";
import { HostService, type KnownHostEntry } from "tddy-client-web/gen/host_pb";
import { hostsScreenPage } from "../support/pages/hostsScreenPage";

const ONLINE_HOST = "workstation-1";
const OFFLINE_HOST = "server-2";

/**
 * The reference time the fixtures are anchored to.
 *
 * It has to be the real clock: the screen renders relative phrases against `Date.now()`, and these
 * tests mount `HostsContent`, which owns that call. A frozen instant here would be compared against
 * wall-clock time and the phrasing would drift with the calendar.
 */
const NOW_MS = Date.now();
const FIVE_MINUTES_MS = 5 * 60 * 1000;

function aKnownHost(overrides: Partial<KnownHostEntry>): KnownHostEntry {
  return {
    instanceId: ONLINE_HOST,
    label: `${ONLINE_HOST} (this daemon)`,
    online: true,
    firstSeenUnixMs: BigInt(NOW_MS - 86_400_000),
    lastSeenUnixMs: BigInt(NOW_MS),
    reposBasePath: "repos",
    maxAttachmentBytes: 0n,
    isLocal: false,
    ...overrides,
  } as KnownHostEntry;
}

function aBackendListing(hosts: KnownHostEntry[]): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(HostService, {
    listKnownHosts: () => ({ hosts }),
  });
}

function mountHosts(backend: InMemoryRpcBackend) {
  cy.mountWithClients(<HostsContent />, backend);
}

describe("Hosts screen", () => {
  it("lists a connected host as online", () => {
    mountHosts(aBackendListing([aKnownHost({ instanceId: ONLINE_HOST, online: true })]));

    hostsScreenPage.row(ONLINE_HOST).should("exist");
    hostsScreenPage.liveness(ONLINE_HOST).should("contain.text", "Online");
  });

  it("lists a previously seen host as offline with its last seen time", () => {
    mountHosts(
      aBackendListing([
        aKnownHost({
          instanceId: OFFLINE_HOST,
          label: `${OFFLINE_HOST} (this daemon)`,
          online: false,
          lastSeenUnixMs: BigInt(NOW_MS - FIVE_MINUTES_MS),
        }),
      ]),
    );

    // The row is present at all — that is the whole point of the registry.
    hostsScreenPage.row(OFFLINE_HOST).should("exist");
    hostsScreenPage.liveness(OFFLINE_HOST).should("contain.text", "Offline");
    hostsScreenPage.lastSeen(OFFLINE_HOST).should("have.text", "5 minutes ago");
  });

  it("sorts online hosts above offline ones", () => {
    mountHosts(
      aBackendListing([
        aKnownHost({ instanceId: OFFLINE_HOST, label: "server-2", online: false }),
        aKnownHost({ instanceId: ONLINE_HOST, label: "workstation-1", online: true }),
      ]),
    );

    hostsScreenPage.rowOrder().should("deep.equal", [ONLINE_HOST, OFFLINE_HOST]);
  });

  // Every daemon self-labels "<id> (this daemon)", so the label cannot carry this fact and the
  // assertion cannot be a substring of it. Only the marker distinguishes the serving host, which is
  // why the negative case below matters as much as the positive one.
  it("marks the host that is serving the page as the local one", () => {
    mountHosts(
      aBackendListing([aKnownHost({ instanceId: ONLINE_HOST, online: true, isLocal: true })]),
    );

    hostsScreenPage.localMarker(ONLINE_HOST).should("be.visible");
  });

  it("leaves the local marker off a host that is not serving the page", () => {
    mountHosts(
      aBackendListing([aKnownHost({ instanceId: OFFLINE_HOST, online: true, isLocal: false })]),
    );

    // The row first: without it the marker would be absent merely because nothing had rendered.
    hostsScreenPage.row(OFFLINE_HOST).should("exist");
    hostsScreenPage.localMarker(OFFLINE_HOST).should("not.exist");
  });

  it("asks the daemon for its known hosts exactly once per visit", () => {
    const backend = aBackendListing([aKnownHost({})]);
    mountHosts(backend);

    hostsScreenPage.row(ONLINE_HOST).should("exist");
    cy.then(() => {
      expect(backend.callsTo(HostService.method.listKnownHosts)).to.have.length(1);
    });
  });
});
