/**
 * Acceptance test: the Hosts page, assembled from the application shell and `tddy-hosts-web`'s
 * data container, is reachable from the shell's navigation menu.
 *
 * Split from `HostsScreenAcceptance` (now in `tddy-hosts-web`) when the screen moved: the menu is the
 * shell's, so only a spec that mounts the shell can prove it offers the entry that asks for this path.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-06-host-registry.md
 */

import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { HostService, type KnownHostEntry } from "tddy-client-web/gen/host_pb";
import { HostsAppPage } from "../../src/components/hosts/HostsAppPage";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { appShellPage } from "../support/pages/appShellPage";
import { hostsScreenPage } from "../support/pages/hostsScreenPage";
import { HOSTS_ROUTE } from "../../src/routing/appRoutes";

const ONLINE_HOST = "workstation-1";

function aKnownHost(): KnownHostEntry {
  return {
    instanceId: ONLINE_HOST,
    label: `${ONLINE_HOST} (this daemon)`,
    online: true,
    firstSeenUnixMs: BigInt(Date.now() - 86_400_000),
    lastSeenUnixMs: BigInt(Date.now()),
    reposBasePath: "repos",
    maxAttachmentBytes: 0n,
    isLocal: false,
  } as KnownHostEntry;
}

describe("Hosts page", () => {
  // AC-1. The hash-route dispatch that turns `/hosts` into this screen is a pure string rule and is
  // pinned in `src/routing/appRoutes.test.ts`; what only the mounted screen can prove is that its
  // own `AppShell` offers the entry that asks for that path.
  it("reaches the hosts screen from the navigation menu", () => {
    const backend = anInMemoryRpcBackend().implement(HostService, { listKnownHosts: () => ({ hosts: [aKnownHost()] }) });
    mountWithRpc(withSelectedDaemon(<HostsAppPage onNavigate={cy.stub().as("onNavigate")} />), backend);
    hostsScreenPage.row(ONLINE_HOST).should("exist");

    appShellPage.openMenu();
    hostsScreenPage.navEntry().click();

    cy.get("@onNavigate").should("have.been.calledWith", HOSTS_ROUTE);
  });
});
