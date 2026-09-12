/**
 * Cypress component acceptance: the Worktrees screen loads through **`worktree.WorktreeService`**.
 *
 * Nine worktree methods left `the pre-unbundle monolithic RPC coordinate` for `worktree.WorktreeService`, and the
 * daemon roster left it for `host.HostService` — but `ListProjects`, which this screen also reads,
 * stayed. So the screen now spans three services, and this spec pins the split: the backend serves
 * the worktree feed and the delete **only** under `worktree.WorktreeService`, the roster only under
 * `host.HostService`, and the project registry only under `the pre-unbundle monolithic RPC coordinate`. A screen
 * that asked any of them of the wrong service gets `Unimplemented` and shows nothing.
 *
 * Changeset: `docs/dev/changesets/2026-09-09-unbundle-host-worktree-services.md`
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { WorktreesAppPage } from "../../src/components/worktrees/WorktreesAppPage";
import { AuthService } from "../../src/gen/auth_pb";
import { ConnectionService, ProjectEntrySchema } from "../../src/gen/connection_pb";
import {
  GenerateTokenResponseSchema,
  RefreshTokenResponseSchema,
  TokenService,
} from "../../src/gen/token_pb";
import { HostService } from "../../src/gen/host_pb";
import { WorktreeService, WorktreeSizeStatus } from "../../src/gen/worktree_pb";
import {
  aHostServiceFake,
  DAEMON_LOCAL,
} from "../support/rpc/hostServiceBackend";
import {
  aWorktreeServiceFake,
  type WorktreeServiceControls,
  type WorktreeStatsRowInput,
} from "../support/rpc/worktreeServiceBackend";
import { ACCESS_TOKEN_KEY, CURRENT_ACCESS_TOKEN } from "../support/rpc/durableSessionBackend";
import { aGitHubUser } from "../support/rpc/responses";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
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
      .implement(HostService, aHostServiceFake({ daemons: [DAEMON_LOCAL] }).handlers)
      // The two services every daemon-mode screen needs to get past the session gate. They are not
      // part of the split; they are here because a signed-out screen renders a login prompt and
      // would make the assertions below vacuous.
      .implement(AuthService, {
        getAuthStatus: async () => ({ authenticated: true, user: aGitHubUser() }),
      })
      .implement(TokenService, {
        generateToken: async () =>
          create(GenerateTokenResponseSchema, { token: "mock-jwt-presence", ttlSeconds: 600n }),
        refreshToken: async () =>
          create(RefreshTokenResponseSchema, { token: "mock-jwt-presence", ttlSeconds: 600n }),
      })
      .implement(ConnectionService, {
        listProjects: async () => ({
          projects: [
            create(ProjectEntrySchema, {
              projectId: PROJECT_ID,
              name: "Demo",
              gitUrl: "https://github.com/test/demo.git",
              mainRepoPath: "/repos/demo",
              daemonInstanceId: DAEMON_LOCAL.instanceId,
            }),
          ],
        }),
      }),
    ...worktreeControls,
  };
}

function mountWorktrees(daemon: ThreeServiceDaemon) {
  mountWithRpc(withSelectedDaemon(<WorktreesAppPage onNavigate={() => undefined} />), daemon.backend);
}

describe("Worktrees screen over worktree.WorktreeService", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    // A real, unexpired access token — the screen gates its content on `isAuthenticated`, and the
    // client-side token gate decodes the token's `exp`. Set through a queued `cy.window()` so it
    // survives the queued `clearLocalStorage()` above.
    cy.window().then((win) => win.localStorage.setItem(ACCESS_TOKEN_KEY, CURRENT_ACCESS_TOKEN));
  });

  it("streams the project's worktrees from the daemon's worktree service", () => {
    // Given a daemon whose worktree feed is served only under `worktree.WorktreeService`
    const daemon = aDaemonServingTheSplit([A_CACHED_WORKTREE]);

    // When the Worktrees manager screen is opened
    mountWorktrees(daemon);

    // Then the streamed worktree is on screen with the size the daemon reported
    page.status(0).should("contain.text", "Cached");
    page.row(0).should("contain.text", "500 MB");
  });

  it("deletes a worktree through the same service", () => {
    // Given the screen showing that worktree
    const daemon = aDaemonServingTheSplit([A_CACHED_WORKTREE]);
    mountWorktrees(daemon);
    page.status(0).should("contain.text", "Cached");

    // When the operator deletes it
    page.deleteBtn(0).click();
    page.confirmDeleteBtn().click();

    // Then `RemoveWorktree` reached the worktree service, addressed at the path on the row
    cy.wrap(null).should(() => {
      expect(daemon.removedWorktreePaths).to.deep.equal([A_CACHED_WORKTREE.path]);
    });
  });
});
