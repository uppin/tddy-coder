/**
 * Acceptance tests: the Worktree Code pane — every session type can open a split Code pane that
 * shows a directory tree of the session's worktree and a read-only file preview.
 *
 * PRD: docs/ft/web/session-code-pane.md.
 *
 * All RPC calls flow through the in-memory backend — no HTTP intercepts.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import {
  ConnectionService,
  ListWorktreeDirectoryResponseSchema,
  ReadWorktreeFileResponseSchema,
  WorktreeDirEntrySchema,
  type ProjectEntry,
} from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { worktreeCodePanePage } from "../support/pages/worktreeCodePanePage";
import { workflowChatScreenPage } from "../support/pages/workflowChatScreenPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const WORKTREE_PATH = "/home/dev/code-pane-project";

function aSession(overrides: { sessionId: string } & Record<string, unknown>) {
  return {
    createdAt: "2026-07-01T09:00:00Z",
    status: "idle",
    repoPath: WORKTREE_PATH,
    pid: 0,
    isActive: false,
    projectId: "proj-code-pane",
    daemonInstanceId: "",
    workflowGoal: "",
    pendingElicitation: false,
    orchestratorSessionId: "",
    recipe: "",
    sessionType: "tool",
    ...overrides,
  };
}

const TERMINAL_SESSION = aSession({
  sessionId: "claude-cli-session-0000-0000-0000-000000000001",
  sessionType: "claude-cli",
  recipe: "",
});

const CHAT_SESSION = aSession({
  sessionId: "tool-session-0000-0000-0000-000000000002",
  sessionType: "tool",
  recipe: "tdd",
});

const PR_STACK_SESSION = aSession({
  sessionId: "pr-stack-session-0000-0000-0000-000000000003",
  sessionType: "tool",
  recipe: "pr-stack",
});

// An "unscoped" session: `projectId` is empty (a real, supported case — see
// `projectForUnscopedSession`), and its worktree is a git worktree *under* a registered project's
// main repo. The project must be resolved from `repoPath` before the worktree RPCs can be called.
const PROJECT_MAIN_REPO = "/home/dev/code-pane-project";
const UNSCOPED_WORKTREE_PATH = "/home/dev/code-pane-project/.worktrees/feature-x";

const CODE_PANE_PROJECT: ProjectEntry = {
  $typeName: "connection.ProjectEntry",
  projectId: "proj-code-pane",
  name: "code-pane-project",
  gitUrl: "https://example.com/dev/code-pane-project.git",
  mainRepoPath: PROJECT_MAIN_REPO,
  daemonInstanceId: "",
};

const UNSCOPED_SESSION = aSession({
  sessionId: "claude-cli-session-0000-0000-0000-000000000004",
  sessionType: "claude-cli",
  recipe: "",
  projectId: "",
  repoPath: UNSCOPED_WORKTREE_PATH,
});

const README_MD = "# Hello Worktree\n\n- alpha\n- beta\n";
const MAIN_RS = 'fn main() { println!("worktree-code-pane"); }\n';

// A worktree with one directory (`src/`) and one root file (`README.md`); `src/` holds `main.rs`.
// The listing is served per directory level (lazy), keyed by the requested `relPath`.
function aWorktreeBackend(sessions: Record<string, unknown>[]) {
  const directories: Record<string, Array<{ name: string; isDir: boolean }>> = {
    "": [
      { name: "src", isDir: true },
      { name: "README.md", isDir: false },
    ],
    src: [{ name: "main.rs", isDir: false }],
  };
  const files: Record<string, string> = {
    "README.md": README_MD,
    "src/main.rs": MAIN_RS,
  };

  return aSessionsDrawerBackend(sessions)
    .onUnary(ConnectionService.method.listWorktreeDirectory, (req) =>
      create(ListWorktreeDirectoryResponseSchema, {
        entries: (directories[req.relPath] ?? []).map((e) => create(WorktreeDirEntrySchema, e)),
      }),
    )
    .onUnary(ConnectionService.method.readWorktreeFile, (req) =>
      create(ReadWorktreeFileResponseSchema, {
        contentUtf8: files[req.relPath] ?? "",
        truncated: false,
        byteSize: BigInt((files[req.relPath] ?? "").length),
      }),
    );
}

// Like `aWorktreeBackend`, but mirrors the daemon's `resolve_listed_worktree` preamble: the worktree
// RPCs reject an empty `project_id` with `[invalid_argument] project_id is required`, and reject a
// `project_id` that is not a registered project with `[not_found]`. Also serves `listProjects` so an
// unscoped session's project can be resolved from its `repoPath`. Entries are only served once the
// correctly-resolved project id reaches the RPC.
function aProjectScopedWorktreeBackend(
  sessions: Record<string, unknown>[],
  projects: ProjectEntry[],
) {
  const directories: Record<string, Array<{ name: string; isDir: boolean }>> = {
    "": [
      { name: "src", isDir: true },
      { name: "README.md", isDir: false },
    ],
    src: [{ name: "main.rs", isDir: false }],
  };
  const files: Record<string, string> = {
    "README.md": README_MD,
    "src/main.rs": MAIN_RS,
  };

  const requireResolvedProject = (projectId: string) => {
    if (projectId.trim() === "") {
      throw new ConnectError("project_id is required", Code.InvalidArgument);
    }
    if (!projects.some((p) => p.projectId === projectId)) {
      throw new ConnectError("project not found", Code.NotFound);
    }
  };

  return aSessionsDrawerBackend(sessions)
    .onUnary(ConnectionService.method.listProjects, () => ({ projects }))
    .onUnary(ConnectionService.method.listWorktreeDirectory, (req) => {
      requireResolvedProject(req.projectId);
      return create(ListWorktreeDirectoryResponseSchema, {
        entries: (directories[req.relPath] ?? []).map((e) => create(WorktreeDirEntrySchema, e)),
      });
    })
    .onUnary(ConnectionService.method.readWorktreeFile, (req) => {
      requireResolvedProject(req.projectId);
      return create(ReadWorktreeFileResponseSchema, {
        contentUtf8: files[req.relPath] ?? "",
        truncated: false,
        byteSize: BigInt((files[req.relPath] ?? "").length),
      });
    });
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800); // desktop: session list defaults open so drawer items are clickable
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Availability — every session type can open the Code pane
// ---------------------------------------------------------------------------

it("opens the split Code pane for a claude-cli terminal session", () => {
  // Given
  const backend = aWorktreeBackend([TERMINAL_SESSION]);
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(TERMINAL_SESSION.sessionId).click();

  // When
  worktreeCodePanePage.toggle().click();

  // Then — the split pane appears alongside the terminal session's base view.
  worktreeCodePanePage.pane().should("exist");
  worktreeCodePanePage.tree().should("exist");
  sessionsDrawerPage.detailPane().should("exist");
});

it("opens the split Code pane for a tool workflow chat session without unmounting the chat", () => {
  // Given
  const backend = aWorktreeBackend([CHAT_SESSION]);
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(CHAT_SESSION.sessionId).click();

  // When
  worktreeCodePanePage.toggle().click();

  // Then — the chat base view stays mounted beside the Code pane.
  worktreeCodePanePage.pane().should("exist");
  workflowChatScreenPage.screen().should("exist");
});

it("opens the split Code pane for a pr-stack session without unmounting the PR-Stack screen", () => {
  // Given
  const backend = aWorktreeBackend([PR_STACK_SESSION]);
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // When
  worktreeCodePanePage.toggle().click();

  // Then — the PR-Stack base view stays mounted beside the Code pane.
  worktreeCodePanePage.pane().should("exist");
  prStackScreenPage.screen().should("exist");
});

// ---------------------------------------------------------------------------
// Directory tree — lazy per-directory listing
// ---------------------------------------------------------------------------

it("lists the worktree root and lazily loads a directory's children when it is expanded", () => {
  // Given
  const backend = aWorktreeBackend([TERMINAL_SESSION]);
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(TERMINAL_SESSION.sessionId).click();
  worktreeCodePanePage.toggle().click();

  // Then — the root level lists the directory and the root file (child not yet fetched).
  worktreeCodePanePage.node("src").should("exist");
  worktreeCodePanePage.node("README.md").should("exist");
  worktreeCodePanePage.node("src/main.rs").should("not.exist");

  // When — the directory is expanded, its children are fetched on demand.
  worktreeCodePanePage.node("src").click();

  // Then
  worktreeCodePanePage.node("src/main.rs").should("exist");
});

// ---------------------------------------------------------------------------
// File preview
// ---------------------------------------------------------------------------

it("renders a selected Markdown file as sanitized markup in the preview", () => {
  // Given
  const backend = aWorktreeBackend([TERMINAL_SESSION]);
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(TERMINAL_SESSION.sessionId).click();
  worktreeCodePanePage.toggle().click();

  // When
  worktreeCodePanePage.node("README.md").click();

  // Then
  worktreeCodePanePage.preview().find("h1").should("contain.text", "Hello Worktree");
});

it("renders a selected code file as monospace text in the preview", () => {
  // Given
  const backend = aWorktreeBackend([TERMINAL_SESSION]);
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(TERMINAL_SESSION.sessionId).click();
  worktreeCodePanePage.toggle().click();
  worktreeCodePanePage.node("src").click();

  // When
  worktreeCodePanePage.node("src/main.rs").click();

  // Then
  worktreeCodePanePage.preview().should("contain.text", 'println!("worktree-code-pane")');
});

// ---------------------------------------------------------------------------
// Toggle closes the pane
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Unscoped sessions — resolve the project from the worktree before the RPC
// ---------------------------------------------------------------------------

it("loads the worktree tree for an unscoped session by resolving its project from the repo path", () => {
  // Given — a session with an empty projectId whose worktree lives under a registered project's
  // main repo; the daemon rejects worktree RPCs that arrive with an empty project_id.
  const backend = aProjectScopedWorktreeBackend([UNSCOPED_SESSION], [CODE_PANE_PROJECT]);
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(UNSCOPED_SESSION.sessionId).click();

  // When
  worktreeCodePanePage.toggle().click();

  // Then — the tree loads (project resolved) instead of surfacing "project_id is required".
  worktreeCodePanePage.node("src").should("exist");
  worktreeCodePanePage.node("README.md").should("exist");
});

it("sends the resolved project id on the worktree directory RPC for an unscoped session", () => {
  // Given
  const backend = aProjectScopedWorktreeBackend([UNSCOPED_SESSION], [CODE_PANE_PROJECT]);
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(UNSCOPED_SESSION.sessionId).click();

  // When
  worktreeCodePanePage.toggle().click();
  worktreeCodePanePage.node("src").should("exist");

  // Then — the directory RPC carried the resolved project id, not the session's empty one.
  cy.then(() => {
    const calls = backend.callsTo(ConnectionService.method.listWorktreeDirectory);
    expect(calls[0].projectId).to.equal(CODE_PANE_PROJECT.projectId);
  });
});

it("collapses the Code pane back to the single base view when toggled off", () => {
  // Given
  const backend = aWorktreeBackend([TERMINAL_SESSION]);
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(TERMINAL_SESSION.sessionId).click();
  worktreeCodePanePage.toggle().click();
  worktreeCodePanePage.pane().should("exist");

  // When
  worktreeCodePanePage.toggle().click();

  // Then
  worktreeCodePanePage.pane().should("not.exist");
});
