/**
 * Cypress component acceptance: the Worktrees screen loads through **`worktree.WorktreeService`**.
 *
 * Nine worktree methods left `session.SessionService` for `worktree.WorktreeService`, and the
 * daemon roster left it for `host.HostService` — but `ListProjects`, which this screen also reads,
 * stayed. So the screen now spans three services, and this spec pins the split: the backend serves
 * the worktree feed and the delete **only** under `worktree.WorktreeService`, the roster from the clients port, and the project registry only under `project.ProjectService`. A screen
 * that asked any of them of the wrong service gets `Unimplemented` and shows nothing.
 *
 * Changeset: `docs/dev/changesets/2026-09-09-unbundle-host-worktree-services.md`
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { anAuthDouble } from "tddy-client-web/testkit";
import { WorktreesContent } from "../../src/worktrees/WorktreesContent";
import { ProjectService, ProjectEntrySchema } from "tddy-client-web/gen/project_pb";
import { WorktreeService, WorktreeSizeStatus } from "tddy-client-web/gen/worktree_pb";
import {
  aWorktreeServiceFake,
  type WorktreeServiceControls,
  type WorktreeStatsRowInput,
} from "../support/rpc/worktreeServiceBackend";
import { worktreesPage as page } from "../support/pages/worktreesPage";

const PROJECT_ID = "proj-1";

const A_CACHED_WORKTREE: WorktreeStatsRowInput = {
  path: "/repos/demo/.worktrees/feat-a",
  branchLabel: "feature/a",
  sizeStatus: WorktreeSizeStatus.CACHED,
  diskBytes: 524_288_000n, // formats as "500 MB"
  sizeCalculatedAtUnixMs: BigInt(Date.now()),
};

interface ThreeServiceDaemon extends WorktreeServiceControls {
  backend: InMemoryRpcBackend;
}

/**
 * A daemon that answers each of the screen's three concerns under the service that now owns it, and
 * under no other.
 */
function aDaemonServingTheSplit(snapshot: WorktreeStatsRowInput[]): ThreeServiceDaemon {
  const { handlers: worktreeHandlers, ...worktreeControls } = aWorktreeServiceFake({
    worktreeStatsSnapshot: snapshot,
  });
  return {
    backend: anInMemoryRpcBackend()
      .implement(WorktreeService, worktreeHandlers)
      // The daemon roster and auth come from the port doubles (`aClientsPortDouble`, `anAuthDouble`),
      // not from RPC: a signed-in auth double keeps the screen from rendering a login prompt, which
      // would make the assertions below vacuous.
      .implement(ProjectService, {
        listProjects: async () => ({
          projects: [
            create(ProjectEntrySchema, {
              projectId: PROJECT_ID,
              name: "Demo",
              gitUrl: "https://github.com/test/demo.git",
              mainRepoPath: "/repos/demo",
              daemonInstanceId: "",
            }),
          ],
        }),
      }),
    ...worktreeControls,
  };
}

function mountWorktrees(daemon: ThreeServiceDaemon) {
  cy.mountWithClients(<WorktreesContent />, daemon.backend, { auth: anAuthDouble({ sessionToken: "session-token" }) });
}

describe("Worktrees screen over worktree.WorktreeService", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("lists the project's worktrees from the daemon's worktree service, by name", () => {
    // Given a daemon whose worktree feed is served only under `worktree.WorktreeService`
    const daemon = aDaemonServingTheSplit([A_CACHED_WORKTREE]);

    // When the Worktrees screen is opened
    mountWorktrees(daemon);

    // Then the worktree is listed by name with the size the daemon reported
    page.rowName(0).should("have.text", "feat-a");
    page.rowSize(0).should("have.text", "500 MB");
  });

  it("shows the selected worktree in the Session Inspector's Worktree pane", () => {
    const daemon = aDaemonServingTheSplit([A_CACHED_WORKTREE]);

    mountWorktrees(daemon);

    page.selectedName().should("have.text", "feat-a");
    page.pane().should("exist");
    page.paneSize().should("contain.text", "500 MB");
  });

  it("deletes a worktree through the same service", () => {
    // Given the screen showing that worktree
    const daemon = aDaemonServingTheSplit([A_CACHED_WORKTREE]);
    mountWorktrees(daemon);
    page.paneSize().should("contain.text", "500 MB");

    // When the operator deletes it from the pane
    page.paneDelete().click();
    page.paneDeleteConfirm().click();

    // Then `RemoveWorktree` reached the worktree service, addressed at the worktree's path
    cy.wrap(null).should(() => {
      expect(daemon.removedWorktreePaths).to.deep.equal([A_CACHED_WORKTREE.path]);
    });
  });
});
