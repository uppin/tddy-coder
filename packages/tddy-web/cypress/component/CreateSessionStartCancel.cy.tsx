/**
 * Acceptance: while a session is created, Cancel stops the start — it cancels the task the start runs
 * as (named by the stream's first event), and when the host ends the stream cancelled the form is
 * editable again, with its values, saying the start was cancelled.
 *
 * Feature docs: docs/ft/web/session-drawer.md (§ Cancel)
 */

import React from "react";
import { Room } from "livekit-client";
import { Code, ConnectError, createClient } from "@connectrpc/connect";
import {
  create,
  type DescMethodUnary,
  type MessageInitShape,
  type MessageShape,
} from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CatalogService } from "../../src/gen/catalog_pb";
import { ProjectService } from "../../src/gen/project_pb";
import {
  SessionService,
  StartPhase_Boundary,
  StartPhase_Step,
  StartSessionEventSchema,
} from "../../src/gen/session_pb";
import { SessionFilesService } from "../../src/gen/session_files_pb";
import { TaskService } from "../../src/gen/tasks_pb";
import { WorktreeService } from "../../src/gen/worktree_pb";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { createSessionPage } from "../support/pages/createSessionPage";

const LOCAL_HOST = "workstation-1";
const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)" },
];
const CLAUDE_OPTION = `claude@${LOCAL_HOST}`;

/** A gate a stub generator awaits, so the UI's mid-stream state is settled when it is asserted. */
interface Gate {
  held: Promise<void>;
  release: () => void;
}

function aGate(): Gate {
  let release: () => void = () => undefined;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { held, release: () => release() };
}

/** A unary handler answering every request with one message built from `init`. */
function answering<M extends DescMethodUnary>(
  method: M,
  init: MessageInitShape<M["output"]>,
): () => MessageShape<M["output"]> {
  return () => create<M["output"]>(method.output, init);
}

/** Every RPC the form issues besides the session start itself. */
function aBaselineBackend(): InMemoryRpcBackend {
  const uploadChunk = SessionFilesService.method.uploadStagedAttachmentChunk;
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, answering(SessionService.method.listSessions, {}))
    .onUnary(
      CatalogService.method.listAgentModels,
      answering(CatalogService.method.listAgentModels, {
        models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
        defaultModel: "claude-opus-4-8",
      }),
    )
    .onUnary(
      ProjectService.method.listProjects,
      answering(ProjectService.method.listProjects, {
        projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
      }),
    )
    .onUnary(
      CatalogService.method.listAgents,
      answering(CatalogService.method.listAgents, { agents: [{ id: "claude", label: "Claude" }] }),
    )
    .onUnary(
      CatalogService.method.listTools,
      answering(CatalogService.method.listTools, {
        tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
      }),
    )
    .onUnary(CatalogService.method.listSubagents, answering(CatalogService.method.listSubagents, {}))
    .onUnary(
      ProjectService.method.listProjectBranches,
      answering(ProjectService.method.listProjectBranches, {
        branches: ["origin/main"],
        defaultRemote: "origin",
      }),
    )
    .onUnary(uploadChunk, (req) =>
      create(uploadChunk.output, {
        entry: req.last
          ? {
              daemonInstanceId: LOCAL_HOST,
              stagingId: req.stagingId,
              fileName: req.fileName,
              hostPath: `/srv/staging/${req.stagingId}/${req.fileName}`,
            }
          : undefined,
      }),
    );
}

function theStartRunsAsTask(taskId: string) {
  return create(StartSessionEventSchema, {
    event: { case: "started", value: { taskId, daemonInstanceId: LOCAL_HOST } },
  });
}

describe("Create session — cancelling a start", () => {
  it("Cancel during a start cancels it and returns to the form saying so", () => {
    // Given a host whose start is cloning until it is cancelled
    const cancelled = aGate();
    const cancelRequests: { taskId: string; daemonInstanceId: string }[] = [];
    const backend = aBaselineBackend()
      .implement(SessionService, {
        async *streamStartSession() {
          yield theStartRunsAsTask("start-task-1");
          yield create(StartSessionEventSchema, {
            event: {
              case: "phase",
              value: { step: StartPhase_Step.CLONE, boundary: StartPhase_Boundary.BEGIN },
            },
          });
          await cancelled.held;
          throw new ConnectError("start cancelled", Code.Canceled);
        },
      })
      .onUnary(TaskService.method.cancelTask, (req) => {
        cancelRequests.push({ taskId: req.taskId, daemonInstanceId: req.daemonInstanceId });
        cancelled.release();
        return create(TaskService.method.cancelTask.output, { ok: true });
      });
    const transport = backend.transport();
    mountWithRpc(
      <SelectedDaemonProvider room={new Room()} daemons={DAEMON_HOSTS} servingInstanceId={LOCAL_HOST}>
        <CreateSessionPane
          client={createClient(SessionService, transport)}
          projectClient={createClient(ProjectService, transport)}
          catalogClient={createClient(CatalogService, transport)}
          sessionFilesClient={createClient(SessionFilesService, transport)}
          worktreeClient={createClient(WorktreeService, transport)}
          taskClient={createClient(TaskService, transport)}
          sessionToken="fake-token"
          onCancel={cy.stub().as("onCancel")}
          onCreated={cy.stub().as("onCreated")}
        />
      </SelectedDaemonProvider>,
      backend,
    );
    createSessionPage.selectProject("proj-1");
    createSessionPage.selectAgent(CLAUDE_OPTION);
    createSessionPage.submit();
    createSessionPage.submitButton().should("be.disabled");

    // When the operator presses Cancel while the start runs
    cy.get("[data-testid='create-session-cancel-btn']").should("not.be.disabled").click();

    // Then the start's task was cancelled on the host doing the work — the pane stays open
    cy.wrap(cancelRequests).should("deep.equal", [
      { taskId: "start-task-1", daemonInstanceId: LOCAL_HOST },
    ]);
    cy.get("@onCancel").should("not.have.been.called");
    cy.get("@onCreated").should("not.have.been.called");

    // And once the host ends the stream cancelled, the form is editable again and says so
    cy.get("[data-testid='create-session-start-cancelled']").should("contain.text", "Start cancelled");
    createSessionPage.error().should("not.exist");
    createSessionPage.submitButton().should("not.be.disabled");
  });
});
