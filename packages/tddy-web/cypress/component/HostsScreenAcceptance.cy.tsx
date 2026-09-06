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
import { HostsAppPage } from "../../src/components/hosts/HostsAppPage";
import { ConnectionService, type KnownHostEntry } from "../../src/gen/connection_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { hostsScreenPage } from "../support/pages/hostsScreenPage";

const ONLINE_HOST = "workstation-1";
const OFFLINE_HOST = "server-2";

/**
 * The reference time the fixtures are anchored to.
 *
 * It has to be the real clock: the screen renders relative phrases against `Date.now()`, and these
 * tests mount `HostsAppPage`, which owns that call. A frozen instant here would be compared against
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
  return anInMemoryRpcBackend().implement(ConnectionService, {
    listKnownHosts: () => ({ hosts }),
  });
}

function mountHosts(backend: InMemoryRpcBackend) {
  mountWithRpc(withSelectedDaemon(<HostsAppPage onNavigate={() => {}} />), backend);
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
    hostsScreenPage.lastSeen(OFFLINE_HOST).should("contain.text", "minute");
  });

  it("sorts online hosts above offline ones", () => {
    mountHosts(
      aBackendListing([
        aKnownHost({ instanceId: OFFLINE_HOST, label: "server-2", online: false }),
        aKnownHost({ instanceId: ONLINE_HOST, label: "workstation-1", online: true }),
      ]),
    );

    hostsScreenPage
      .rows()
      .then(($rows) => {
        const ids = [...$rows].map((el) => el.getAttribute("data-testid"));
        expect(ids).to.deep.equal([
          `hosts-row-${ONLINE_HOST}`,
          `hosts-row-${OFFLINE_HOST}`,
        ]);
      });
  });

  it("shows the host that is serving the page as the local one", () => {
    mountHosts(
      aBackendListing([aKnownHost({ instanceId: ONLINE_HOST, online: true, isLocal: true })]),
    );

    hostsScreenPage.row(ONLINE_HOST).should("contain.text", "this daemon");
  });

  it("asks the daemon for its known hosts exactly once per visit", () => {
    const backend = aBackendListing([aKnownHost({})]);
    mountHosts(backend);

    hostsScreenPage.row(ONLINE_HOST).should("exist");
    cy.then(() => {
      expect(backend.callsTo(ConnectionService.method.listKnownHosts)).to.have.length(1);
    });
  });
});
