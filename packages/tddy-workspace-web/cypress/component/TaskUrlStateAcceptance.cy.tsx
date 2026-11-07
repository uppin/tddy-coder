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
import { aNavigationDouble, anAuthDouble } from "tddy-client-web/testkit";
import { TaskStatusProto } from "tddy-client-web/gen/tasks_pb";
import { TasksScreen } from "../../src/tasks/TasksScreen";
import { aTaskInfo, aTaskServiceBackend, snapshotTaskAdded } from "../support/rpc/taskRpcs";
import { tasksDrawerPage } from "../support/pages/tasksDrawerPage";
import { navigationPage } from "../support/pages/navigationPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

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

/** The navigation double the current test mounted into; `startAt` replaces it before mounting. */
let navigation = aNavigationDouble("/tasks");
let appLocationPage = navigationPage(navigation);

/** Put the app at `path` (a deep link) before mounting. */
function startAt(path: string) {
  navigation = aNavigationDouble(path);
  appLocationPage = navigationPage(navigation);
}

/** Mount the tasks drawer with both fixture tasks in the initial `WatchTaskList` snapshot. */
function mountTasksDrawer() {
  const backend = aTaskServiceBackend({
    watchTaskListEvents: [snapshotTaskAdded(BUILD_TASK), snapshotTaskAdded(LINT_TASK)],
    watchTaskOutput: "compiling…",
  });
  cy.mountWithClients(<TasksScreen />, backend, {
    navigation,
    auth: anAuthDouble({ sessionToken: "fake-token" }),
  });
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  startAt("/tasks");
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
  startAt(`/tasks/${LINT_TASK.taskId}`);

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
  startAt(`/tasks/${BUILD_TASK.taskId}?channel=1`);

  // When
  mountTasksDrawer();

  // Then
  tasksDrawerPage.channelOutput(BUILD_TASK.taskId, "1").should("exist");
});

it("an unknown ?channel= value falls back to the task's first channel", () => {
  // Given — a channel id the task does not have
  startAt(`/tasks/${LINT_TASK.taskId}?channel=99`);

  // When
  mountTasksDrawer();

  // Then
  tasksDrawerPage.channelOutput(LINT_TASK.taskId, "0").should("exist");
});
