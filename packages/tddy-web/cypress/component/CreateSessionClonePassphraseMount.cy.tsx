/**
 * Acceptance tests: the create-session form puts a host's passphrase question in front of the
 * operator while their start is waiting on it — and listens to the host that is doing the cloning.
 *
 * `StartPassphrasePrompt` itself is pinned in `CreateSessionClonePassphraseAcceptance.cy.tsx`. What
 * only the real pane can show is the wiring: that a submitted start opens the feed, that the dialog
 * names the right host, and that the pane — which is handed its session token as a prop and is
 * mounted here without the auth context — still renders.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-10-17-clone-passphrase-prompt.md (AC12)
 */

import React from "react";
import { Room } from "livekit-client";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { SessionService } from "../../src/gen/session_pb";
import { ProjectService } from "../../src/gen/project_pb";
import { CatalogService } from "../../src/gen/catalog_pb";
import { SessionFilesService } from "../../src/gen/session_files_pb";
import { WorktreeService } from "../../src/gen/worktree_pb";
import { HostService } from "../../src/gen/host_pb";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aHostPromptFeed, type HostPromptFeed } from "../support/rpc/hostPromptFeed";
import { anRsaOaepKeypair, fingerprintOf } from "../support/hostKeys";
import { createSessionPage } from "../support/pages/createSessionPage";
import { hostPassphraseDialogPage as dialog } from "../support/pages/hostsScreenPage";

/** The daemon running the agent — and, for a co-located start, the one that clones. */
const AGENT_HOST = "laptop-a";
/** The daemon holding the checkout — the one that clones for a split start. */
const CODEBASE_HOST = "workstation-b";
const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: AGENT_HOST, label: "laptop-a (this daemon)" },
  { instanceId: CODEBASE_HOST, label: "workstation-b" },
];
const KEY_ON_THE_HOST = "/home/mantas/.ssh/id_rsa";

let hostPublicKey: Uint8Array;
let hostFingerprint: string;

/** A host whose start never finishes: it is waiting on something, which is the state under test. */
function aBackendWhoseStartWaits(feed: HostPromptFeed): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(CatalogService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(CatalogService.method.listAgents, () => ({
      agents: [{ id: "claude", label: "Claude" }],
    }))
    .onUnary(CatalogService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({
      branches: ["origin/main"],
      defaultRemote: "origin",
    }))
    .implement(SessionService, {
      // eslint-disable-next-line require-yield
      async *streamStartSession() {
        await new Promise<never>(() => undefined);
      },
    })
    .implement(HostService, {
      ...feed.handlers,
      answerHostPrompt: async () => ({ accepted: true, rejectionReason: "" }),
    });
}

/** The pane as production mounts it: handed its token as a prop, with no auth context around it. */
function mountCreatePane(backend: InMemoryRpcBackend) {
  mountWithRpc(
    <SelectedDaemonProvider room={new Room()} daemons={DAEMON_HOSTS} servingInstanceId={AGENT_HOST}>
      <CreateSessionPane
        client={createClient(SessionService, backend.transport())}
        projectClient={createClient(ProjectService, backend.transport())}
        catalogClient={createClient(CatalogService, backend.transport())}
        sessionFilesClient={createClient(SessionFilesService, backend.transport())}
        worktreeClient={createClient(WorktreeService, backend.transport())}
        sessionToken="fake-token"
        onCancel={cy.stub()}
        onCreated={cy.stub()}
      />
    </SelectedDaemonProvider>,
    backend,
  );
}

function theHostAsksForAPassphrase(feed: HostPromptFeed, host: string) {
  // The feed opens when the start is submitted and not before; a prompt raised earlier reaches nobody.
  cy.wrap(null).should(() => expect(feed.subscriptionCount()).to.equal(1));
  cy.then(() =>
    feed.raise({
      promptId: "prompt-1",
      daemonInstanceId: host,
      subject: KEY_ON_THE_HOST,
      hostPublicKey,
      hostPublicKeyFingerprint: hostFingerprint,
    }),
  );
}

describe("Passphrase prompt on the create-session form", () => {
  before(() => {
    cy.wrap(anRsaOaepKeypair()).then(async (keypair) => {
      hostPublicKey = (keypair as { spkiDer: Uint8Array }).spkiDer;
      hostFingerprint = await fingerprintOf(hostPublicKey);
    });
  });

  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearAllSessionStorage();
    cy.clearLocalStorage();
  });

  it("opens no prompt feed until a start is submitted", () => {
    // Given a form nobody has submitted
    const feed = aHostPromptFeed();
    mountCreatePane(aBackendWhoseStartWaits(feed));
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.selectProject("proj-1");

    // Then no stream was opened and no dialog is shown
    cy.then(() => expect(feed.subscriptionCount()).to.equal(0));
    dialog.root(AGENT_HOST).should("not.exist");
  });

  it("asks for the passphrase of the session's own host while a co-located start waits", () => {
    // Given a start that is waiting on its host
    const feed = aHostPromptFeed();
    mountCreatePane(aBackendWhoseStartWaits(feed));
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.selectProject("proj-1");
    createSessionPage.submit();

    // When the host asks for a key's passphrase
    theHostAsksForAPassphrase(feed, AGENT_HOST);

    // Then the operator is asked, naming that host and the key
    dialog.root(AGENT_HOST).should("contain.text", KEY_ON_THE_HOST);
  });

  it("asks the codebase host, not the agent host, when the codebase is placed elsewhere", () => {
    // Given a split start: the agent runs on one host, the worktree is cloned on another
    const feed = aHostPromptFeed();
    mountCreatePane(aBackendWhoseStartWaits(feed));
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.selectProject("proj-1");
    createSessionPage.enableManagedCodebase();
    createSessionPage.selectCodebaseHost(CODEBASE_HOST);
    createSessionPage.submit();

    // When the host that is cloning asks for a passphrase
    theHostAsksForAPassphrase(feed, CODEBASE_HOST);

    // Then the dialog is the codebase host's, and the agent host is not the one being asked
    dialog.root(CODEBASE_HOST).should("contain.text", KEY_ON_THE_HOST);
    dialog.root(AGENT_HOST).should("not.exist");
  });
});
