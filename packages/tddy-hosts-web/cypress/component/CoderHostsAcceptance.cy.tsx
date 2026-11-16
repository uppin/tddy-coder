/**
 * Acceptance tests: the Hosts screen lists the workspaces of a person's Coder accounts beneath the
 * daemon that reaches them, named with their account, and says whether each can run a session.
 *
 * A workspace that is not running is listed too: hiding it would read as an account that has no
 * workspaces. Its row says it is unavailable, and why, in the daemon's own words.
 *
 * The backend implements `host.HostService` only: the known-hosts listing and `ListDerivedHosts`.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-coder-hosts.md
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  DerivedHostKind,
  DerivedHostSchema,
  HostService,
  ListDerivedHostsResponseSchema,
  ProbeOutcome,
  type DerivedHost,
  type KnownHostEntry,
} from "tddy-client-web/gen/host_pb";
import { HostsContent } from "../../src/HostsContent";
import { derivedHostRowTestId } from "../../src/DerivedHostRows";
import { derivedHostsPage } from "../support/pages/hostsScreenPage";

const PARENT = "workstation-1";
/** The person's Coder account, as the accounts listing names it. */
const WORK_ACCOUNT = "coder-3f1c9a52@coder.example.com";
const DEVBOX = `${PARENT}/coder/${WORK_ACCOUNT}/ada/devbox`;
const SCRATCH = `${PARENT}/coder/${WORK_ACCOUNT}/ada/scratch`;
/** The directory lists the daemon its rows are about, so the screen has a connection to ask over. */
const PARENT_LISTED = [{ instanceId: PARENT, label: PARENT }];

/** The daemon the workspaces hang off: online, and the one serving the page. */
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

/** `ada`'s workspace `workspace` of the Work account, as the parent lists it. */
function aWorkspace(workspace: string, usability: { usable: boolean; unusableReason?: string }): DerivedHost {
  return create(DerivedHostSchema, {
    id: `${PARENT}/coder/${WORK_ACCOUNT}/ada/${workspace}`,
    label: `${workspace} (Work)`,
    kind: DerivedHostKind.CODER,
    parentInstanceId: PARENT,
    target: {
      kind: { case: "coder", value: { accountId: WORK_ACCOUNT, owner: "ada", workspace } },
    },
    usable: usability.usable,
    unusableReason: usability.unusableReason ?? "",
  });
}

/** A workspace that is running, on a daemon that has the `coder` CLI. */
function aRunningWorkspace(workspace: string): DerivedHost {
  return aWorkspace(workspace, { usable: true });
}

/** A workspace its deployment reports as stopped. */
function aStoppedWorkspace(workspace: string): DerivedHost {
  return aWorkspace(workspace, { usable: false, unusableReason: "the workspace is stopped" });
}

/** A daemon that knows itself and lists `hosts` as the hosts it reaches. */
function aDaemonReaching(hosts: DerivedHost[]): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(HostService, {
    listKnownHosts: () => ({ hosts: [theParentDaemon()] }),
    listDerivedHosts: async () =>
      create(ListDerivedHostsResponseSchema, { outcome: ProbeOutcome.OK, hosts }),
  });
}

function mountHosts(backend: InMemoryRpcBackend) {
  cy.mountWithClients(<HostsContent />, backend, { daemons: PARENT_LISTED });
}

describe("Hosts screen Coder workspaces", () => {
  it("lists a Coder workspace beneath its daemon with its account and status", () => {
    // Given a daemon that reaches the running workspace devbox of the Work account
    const backend = aDaemonReaching([aRunningWorkspace("devbox")]);

    // When
    mountHosts(backend);

    // Then the workspace is a row of its own directly beneath the daemon's
    derivedHostsPage
      .tableRowTestIds()
      .should("deep.equal", [`hosts-row-${PARENT}`, derivedHostRowTestId(DEVBOX)]);
    // And it says which workspace of which account it is, what it is reached through, and that a
    // session can run on it
    derivedHostsPage.row(DEVBOX).should("contain.text", "devbox (Work)");
    derivedHostsPage.kind(DEVBOX).should("have.text", "Coder");
    derivedHostsPage.status(DEVBOX).should("have.text", "Available");
    derivedHostsPage.unusableReason(DEVBOX).should("not.exist");
  });

  it("marks a stopped workspace and says why it cannot be used", () => {
    // Given a daemon that reaches one running and one stopped workspace
    const backend = aDaemonReaching([aRunningWorkspace("devbox"), aStoppedWorkspace("scratch")]);

    // When
    mountHosts(backend);

    // Then the stopped workspace is listed beside the running one, not hidden
    derivedHostsPage
      .tableRowTestIds()
      .should("deep.equal", [
        `hosts-row-${PARENT}`,
        derivedHostRowTestId(DEVBOX),
        derivedHostRowTestId(SCRATCH),
      ]);
    // And its row says it is unavailable, and why, in the daemon's words
    derivedHostsPage.status(SCRATCH).should("have.text", "Unavailable");
    derivedHostsPage.unusableReason(SCRATCH).should("have.text", "the workspace is stopped");
    // And the running one is not marked with it
    derivedHostsPage.status(DEVBOX).should("have.text", "Available");
  });
});
