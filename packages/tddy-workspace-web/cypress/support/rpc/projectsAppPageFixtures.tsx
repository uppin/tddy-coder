/**
 * Fixtures for specs that mount the Projects screen with its data container (`ProjectsContent`):
 * the hosts it can choose between, a project row, the Projects RPCs answering normally, and the
 * mount wrapper with the providers the page reads.
 */

import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ProjectsContent } from "../../../src/projects/ProjectsContent";
import type { Transport } from "@connectrpc/connect";
import { ConnectionProviderRegistry, type ClientsPort } from "tddy-client-web";
import { aClientsPortDouble, anAuthDouble } from "tddy-client-web/testkit";
import { aHostConnection } from "./hostConnections";
import { ProjectService, type ProjectEntry } from "tddy-client-web/gen/project_pb";
import { AccountsService } from "tddy-client-web/gen/accounts_pb";
import type { DaemonHost } from "tddy-client-web";

/** The host that served the page, and the one selected. */
export const PROJECTS_LOCAL_HOST = "workstation-1";
/** Another listed host, for the add-to-host flows. */
export const PROJECTS_REMOTE_HOST = "server-2";

/** Both hosts, each advertising itself the way a daemon does. */
export const PROJECTS_DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: PROJECTS_LOCAL_HOST, label: "workstation-1 (this daemon)" },
  { instanceId: PROJECTS_REMOTE_HOST, label: "server-2" },
];

export function aProject(overrides: Partial<ProjectEntry> = {}): ProjectEntry {
  return {
    projectId: "proj-alpha",
    name: "alpha",
    gitUrl: "https://example.com/alpha.git",
    mainRepoPath: "/home/dev/repos/alpha",
    daemonInstanceId: PROJECTS_LOCAL_HOST,
    mainBranchRef: "",
    defaultRemote: "",
    accounts: [],
    ...overrides,
  } as ProjectEntry;
}

/**
 * The Projects RPCs answering normally — one project, its branches, a create that adds a row, and an
 * empty vault — with `configure` applied last: a method registered twice is answered by its last
 * registration, so a failure set there replaces the default.
 */
export function aProjectsBackend(
  configure: (b: InMemoryRpcBackend) => InMemoryRpcBackend = (b) => b,
): InMemoryRpcBackend {
  const projects = [aProject()];
  const backend = anInMemoryRpcBackend()
    .onUnary(ProjectService.method.listProjects, () => ({ projects }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({
      branches: ["origin/main"],
      defaultRemote: "origin",
    }))
    .onUnary(ProjectService.method.createProject, (req) => {
      const project = aProject({ projectId: "proj-new", name: req.name });
      projects.push(project);
      return { project };
    })
    .onUnary(AccountsService.method.listAccounts, () => ({ providers: [] }) as never);
  return configure(backend);
}

/** A transport whose every call fails with `reason` — a wire to a room that is not connected. */
function aTransportFailingWith(reason: string): Transport {
  const fail = async (): Promise<never> => {
    throw new Error(reason);
  };
  return { unary: fail, stream: fail } as Transport;
}

/**
 * The Projects screen under the ports it reads. `wire` states how the selected host is reached:
 * `"served"` over `backend` (default), `"none"` (no connection reaches any host), or
 * `{ failingWith }` (every call fails with that reason — what a room that never connected says).
 */
export function mountProjectsContent(
  backend: InMemoryRpcBackend,
  wire: "served" | "none" | { failingWith: string } = "served",
) {
  const double = aClientsPortDouble(backend, {
    daemons: PROJECTS_DAEMON_HOSTS,
    selectedInstanceId: PROJECTS_LOCAL_HOST,
  });
  let clients: ClientsPort = double;
  if (wire === "none") {
    clients = { ...double, connections: new ConnectionProviderRegistry() };
  } else if (wire !== "served") {
    const failing = aTransportFailingWith(wire.failingWith);
    const connections = new ConnectionProviderRegistry();
    connections.register({
      id: "failing",
      connectHost: (hostId) => aHostConnection(hostId).servingOver(failing).build(),
    });
    clients = { ...double, connections, httpTransport: failing };
  }
  return cy.mountWithClients(<ProjectsContent />, backend, { clients, auth: anAuthDouble({ sessionToken: "fake-token" }) });
}
