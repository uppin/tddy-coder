/**
 * Acceptance tests: the tasks drawer's selected task and output-channel tab round-trip through the
 * URL — selecting a task is a navigation, not hidden component state.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-url-state-routing.md.
 * Changeset: docs/dev/1-WIP/2026-08-01-web-url-state-routing.md.
 *
 * All RPC calls flow through the in-memory backend — no HTTP intercepts.
 */

import React from "react";
import { Room } from "livekit-client";
import { TaskStatusProto } from "../../src/gen/tasks_pb";
import { TasksDrawerScreen } from "../../src/components/tasks/TasksDrawerScreen";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../src/hooks/authProvider";
import { aTaskInfo, aTaskServiceBackend, snapshotTaskAdded } from "../support/rpc/taskRpcs";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { tasksDrawerPage } from "../support/pages/tasksDrawerPage";
import { appLocationPage } from "../support/pages/appLocationPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const DAEMON: DaemonHost = { instanceId: "udoo", label: "udoo (this daemon)" };

const BUILD_TASK = aTaskInfo({
  taskId: "task-build-0000-0000-0000-000000000001",
  kind: "shell",
  status: TaskStatusProto.TASK_STATUS_RUNNING,
  channels: [
    { channelId: "0", name: "stdout" },
    { channelId: "1", name: "stderr" },
  ],
});

const LINT_TASK = aTaskInfo({
  taskId: "task-lint-00000-0000-0000-000000000002",
  kind: "execute_tool:Lint",
  status: TaskStatusProto.TASK_STATUS_COMPLETED,
  channels: [{ channelId: "0", name: "stdout" }],
});

/** Mount the tasks drawer with both fixture tasks in the initial `WatchTaskList` snapshot. */
function mountTasksDrawer() {
  const backend = aTaskServiceBackend({
    watchTaskListEvents: [snapshotTaskAdded(BUILD_TASK), snapshotTaskAdded(LINT_TASK)],
    watchTaskOutput: "compiling…",
  });
  mountWithRecordingLiveKitRpc(
    <AuthProvider>
      <SelectedDaemonProvider room={new Room()} daemons={[DAEMON]}>
        <TasksDrawerScreen />
      </SelectedDaemonProvider>
    </AuthProvider>,
    backend,
  );
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  window.location.hash = "/tasks";
});

// ---------------------------------------------------------------------------
// Task selection
// ---------------------------------------------------------------------------

it("selecting a task puts its id in the URL", () => {
  // Given
  mountTasksDrawer();

  // When
  tasksDrawerPage.drawerItem(BUILD_TASK.taskId).click();

  // Then
  appLocationPage.expectPath(`/tasks/${BUILD_TASK.taskId}`);
});

it("a #/tasks/:taskId deep link opens that task's output pane on load", () => {
  // Given
  appLocationPage.startAt(`/tasks/${LINT_TASK.taskId}`);

  // When
  mountTasksDrawer();

  // Then
  tasksDrawerPage.outputPane().should("exist");
  tasksDrawerPage.outputPaneEmpty().should("not.exist");
  tasksDrawerPage.channelOutput(LINT_TASK.taskId, "0").should("exist");
});

it("going back after selecting a second task re-selects the first", () => {
  // Given
  mountTasksDrawer();
  tasksDrawerPage.drawerItem(BUILD_TASK.taskId).click();
  appLocationPage.expectPath(`/tasks/${BUILD_TASK.taskId}`);
  tasksDrawerPage.drawerItem(LINT_TASK.taskId).click();
  appLocationPage.expectPath(`/tasks/${LINT_TASK.taskId}`);

  // When
  appLocationPage.goBack();

  // Then
  appLocationPage.expectPath(`/tasks/${BUILD_TASK.taskId}`);
  tasksDrawerPage.channelOutput(BUILD_TASK.taskId, "0").should("exist");
});

// ---------------------------------------------------------------------------
// Output channel tab
// ---------------------------------------------------------------------------

it("choosing an output channel records it in the URL", () => {
  // Given — a task with stdout and stderr channels, opened on the first
  mountTasksDrawer();
  tasksDrawerPage.drawerItem(BUILD_TASK.taskId).click();

  // When
  tasksDrawerPage.channelTab(BUILD_TASK.taskId, "1").click();

  // Then
  appLocationPage.expectParam("channel", "1");
});

it("a ?channel= deep link opens that output channel on load", () => {
  // Given
  appLocationPage.startAt(`/tasks/${BUILD_TASK.taskId}?channel=1`);

  // When
  mountTasksDrawer();

  // Then
  tasksDrawerPage.channelOutput(BUILD_TASK.taskId, "1").should("exist");
});

it("an unknown ?channel= value falls back to the task's first channel", () => {
  // Given — a channel id the task does not have
  appLocationPage.startAt(`/tasks/${LINT_TASK.taskId}?channel=99`);

  // When
  mountTasksDrawer();

  // Then
  tasksDrawerPage.channelOutput(LINT_TASK.taskId, "0").should("exist");
});
