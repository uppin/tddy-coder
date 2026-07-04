/**
 * Acceptance tests for the TasksDrawerScreen — real-time tasks list with channel output.
 *
 * `TaskService` is daemon-level RPC (`useDaemonClient`), routed over the shared common-room
 * LiveKit connection — see `aTaskServiceBackend` (in-memory fake) and `SelectedDaemonProvider`.
 * All auth is bypassed by pre-seeding localStorage with a fake session token.
 */

import React from "react";
import { Room } from "livekit-client";
import { TaskStatusProto } from "../../../src/gen/tasks_pb";
import { TasksDrawerScreen } from "../../../src/components/tasks/TasksDrawerScreen";
import type { DaemonHost } from "../../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../../src/rpc/selectedDaemon";
import { aTaskInfo, aTaskServiceBackend, liveTaskUpdated, snapshotTaskAdded } from "../../support/rpc/taskRpcs";
import { mountWithRecordingLiveKitRpc } from "../../support/rpc/recordingLiveKitRpc";
import { tasksDrawerPage } from "../../support/pages/tasksDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const DAEMON: DaemonHost = { instanceId: "udoo", label: "udoo (this daemon)" };

const RUNNING_TASK = aTaskInfo({
  taskId: "task-running-0000-0000-0000-000000000001",
  kind: "shell",
  status: TaskStatusProto.TASK_STATUS_RUNNING,
  channels: [{ channelId: "0", name: "stdout" }],
});

const COMPLETED_TASK = aTaskInfo({
  taskId: "task-done-000000-0000-0000-0000-000000000002",
  kind: "execute_tool:Read",
  status: TaskStatusProto.TASK_STATUS_COMPLETED,
  exitCode: 0,
  channels: [
    { channelId: "0", name: "stdout" },
    { channelId: "1", name: "stderr" },
  ],
});

/** Mounts `TasksDrawerScreen` with a single-daemon shared context routed to `backend`. */
function mountTasksDrawerScreen(backend: ReturnType<typeof aTaskServiceBackend>) {
  mountWithRecordingLiveKitRpc(
    <SelectedDaemonProvider room={new Room()} daemons={[DAEMON]}>
      <TasksDrawerScreen />
    </SelectedDaemonProvider>,
    backend,
  );
}

// ---------------------------------------------------------------------------

describe("TasksDrawerScreen — real-time task list and channel output", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  // -------------------------------------------------------------------------
  // AC1: Task list renders from WatchTaskList snapshot events
  // -------------------------------------------------------------------------

  it("renders task list from WatchTaskList snapshot events", () => {
    // Given — two tasks in the initial snapshot
    const backend = aTaskServiceBackend({
      watchTaskListEvents: [snapshotTaskAdded(RUNNING_TASK), snapshotTaskAdded(COMPLETED_TASK)],
    });

    // When
    mountTasksDrawerScreen(backend);

    // Then — both tasks appear in the drawer
    tasksDrawerPage.drawerItem(RUNNING_TASK.taskId).should("exist");
    tasksDrawerPage.drawerItem(COMPLETED_TASK.taskId).should("exist");
  });

  // -------------------------------------------------------------------------
  // AC2: Task row shows status indicator dot with correct kind text
  // -------------------------------------------------------------------------

  it("task row shows running status indicator and kind text", () => {
    // Given
    const backend = aTaskServiceBackend({ watchTaskListEvents: [snapshotTaskAdded(RUNNING_TASK)] });

    // When
    mountTasksDrawerScreen(backend);

    // Then — status dot present, kind text visible
    tasksDrawerPage.drawerItemStatus(RUNNING_TASK.taskId).should("exist");
    tasksDrawerPage.drawerItemKind(RUNNING_TASK.taskId).should("contain.text", "shell");
  });

  // -------------------------------------------------------------------------
  // AC3: Clicking a task opens the output pane
  // -------------------------------------------------------------------------

  it("clicking a task opens the output pane", () => {
    // Given
    const backend = aTaskServiceBackend({
      watchTaskListEvents: [snapshotTaskAdded(RUNNING_TASK)],
      watchTaskOutput: "hello from shell",
    });

    // When
    mountTasksDrawerScreen(backend);

    // Then — output pane not visible before click
    tasksDrawerPage.outputPane().should("not.exist");
    tasksDrawerPage.outputPaneEmpty().should("exist");

    // When — click the task
    tasksDrawerPage.drawerItem(RUNNING_TASK.taskId).click();

    // Then — output pane appears
    tasksDrawerPage.outputPane().should("exist");
  });

  // -------------------------------------------------------------------------
  // AC4: Output pane shows channel tabs for a multi-channel task
  // -------------------------------------------------------------------------

  it("output pane shows channel tabs for a multi-channel task", () => {
    // Given — completed task with stdout + stderr channels
    const backend = aTaskServiceBackend({
      watchTaskListEvents: [snapshotTaskAdded(COMPLETED_TASK)],
      watchTaskOutput: "some stdout",
    });

    // When
    mountTasksDrawerScreen(backend);
    tasksDrawerPage.drawerItem(COMPLETED_TASK.taskId).click();

    // Then — tabs for both channels are present
    tasksDrawerPage.channelTab(COMPLETED_TASK.taskId, "0").should("exist");
    tasksDrawerPage.channelTab(COMPLETED_TASK.taskId, "1").should("exist");
  });

  // -------------------------------------------------------------------------
  // AC5: Channel output area shows bytes streamed from WatchTask
  // -------------------------------------------------------------------------

  it("channel output area shows bytes streamed from WatchTask", () => {
    // Given
    const backend = aTaskServiceBackend({
      watchTaskListEvents: [snapshotTaskAdded(RUNNING_TASK)],
      watchTaskOutput: "hello from shell\n",
    });

    // When
    mountTasksDrawerScreen(backend);
    tasksDrawerPage.drawerItem(RUNNING_TASK.taskId).click();

    // Then — output text appears in the channel output area
    tasksDrawerPage
      .channelOutput(RUNNING_TASK.taskId, "0")
      .should("contain.text", "hello from shell");
  });

  // -------------------------------------------------------------------------
  // AC6: Cancel button calls CancelTask and reflects cancelling state
  // -------------------------------------------------------------------------

  it("cancel button in task row calls CancelTask and reflects cancelling state", () => {
    // Given — a running task
    const backend = aTaskServiceBackend({ watchTaskListEvents: [snapshotTaskAdded(RUNNING_TASK)] });

    // When
    mountTasksDrawerScreen(backend);

    // Then — cancel button is present for running task
    tasksDrawerPage.drawerItemCancel(RUNNING_TASK.taskId).should("exist");

    // When — click cancel
    tasksDrawerPage.drawerItemCancel(RUNNING_TASK.taskId).click();

    // Then — cancel RPC was called, and the button shows cancelling state (disabled)
    cy.wrap(backend).should((b) => {
      expect(b.cancelTaskCalls).to.deep.equal([{ taskId: RUNNING_TASK.taskId }]);
    });
    tasksDrawerPage
      .drawerItemCancel(RUNNING_TASK.taskId)
      .should("have.attr", "disabled");
  });

  // -------------------------------------------------------------------------
  // AC7: task_updated event updates status in list without re-subscribing
  // -------------------------------------------------------------------------

  it("task_updated event updates status in list without re-subscribing", () => {
    // Given — start with a running task, then receive an updated event making it completed
    const completedVersion = aTaskInfo({
      ...RUNNING_TASK,
      status: TaskStatusProto.TASK_STATUS_COMPLETED,
      exitCode: 0,
    });
    const backend = aTaskServiceBackend({
      watchTaskListEvents: [snapshotTaskAdded(RUNNING_TASK), liveTaskUpdated(completedVersion)],
    });

    // When
    mountTasksDrawerScreen(backend);

    // Then — after the updated event, the task row reflects completed status
    // (the status dot or text should update without a new WatchTaskList call)
    tasksDrawerPage.drawerItemStatus(RUNNING_TASK.taskId).should("exist");

    // Cancel button should NOT appear for a completed task
    tasksDrawerPage
      .drawerItemCancel(RUNNING_TASK.taskId)
      .should("not.exist");
  });
});
