/**
 * In-memory `tasks.TaskService` backend + fixtures for the TasksDrawerScreen acceptance tests.
 *
 * `TaskService` is daemon-level RPC (`useDaemonClient`, see `../../../src/rpc/selectedDaemon`),
 * so tests route it through `anInMemoryRpcBackend()` + `mountWithRecordingLiveKitRpc` — the
 * fluent-tests-preferred in-memory fake — rather than wire-level `cy.intercept`, which can only
 * see HTTP traffic and would never observe LiveKit-transport RPC.
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  CancelTaskResponseSchema,
  TaskChannelInfoSchema,
  TaskInfoSchema,
  TaskListEventSchema,
  TaskOutputEventSchema,
  TaskService,
  TaskStatusProto,
  ChannelKindProto,
  type TaskInfo,
  type TaskListEvent,
} from "../../../src/gen/tasks_pb";

// ---------------------------------------------------------------------------
// TaskInfo factory
// ---------------------------------------------------------------------------

export interface TaskInfoOverrides {
  taskId?: string;
  kind?: string;
  status?: TaskStatusProto;
  exitCode?: number;
  errorMessage?: string;
  createdUnixMs?: bigint;
  channels?: Array<{ channelId: string; name: string; kind?: ChannelKindProto; acceptsInput?: boolean }>;
}

export function aTaskInfo(overrides: TaskInfoOverrides = {}): TaskInfo {
  return create(TaskInfoSchema, {
    taskId: overrides.taskId ?? "task-00000000-0000-0000-0000-000000000001",
    kind: overrides.kind ?? "shell",
    status: overrides.status ?? TaskStatusProto.TASK_STATUS_RUNNING,
    exitCode: overrides.exitCode ?? 0,
    errorMessage: overrides.errorMessage ?? "",
    createdUnixMs: overrides.createdUnixMs ?? BigInt(Date.now() - 60_000),
    channels: (overrides.channels ?? [{ channelId: "0", name: "stdout" }]).map((ch) =>
      create(TaskChannelInfoSchema, {
        channelId: ch.channelId,
        name: ch.name,
        kind: ch.kind ?? ChannelKindProto.CHANNEL_KIND_COMBINED,
        acceptsInput: ch.acceptsInput ?? false,
      })
    ),
  });
}

/** A snapshot `task_added` event (part of `WatchTaskList`'s initial snapshot). */
export function snapshotTaskAdded(task: TaskInfo): TaskListEvent {
  return create(TaskListEventSchema, {
    isSnapshot: true,
    event: { case: "taskAdded", value: task },
  });
}

/** A live (post-snapshot) `task_added` event. */
export function liveTaskAdded(task: TaskInfo): TaskListEvent {
  return create(TaskListEventSchema, {
    isSnapshot: false,
    event: { case: "taskAdded", value: task },
  });
}

/** A live (post-snapshot) `task_updated` event. */
export function liveTaskUpdated(task: TaskInfo): TaskListEvent {
  return create(TaskListEventSchema, {
    isSnapshot: false,
    event: { case: "taskUpdated", value: task },
  });
}

// ---------------------------------------------------------------------------
// In-memory TaskService backend
// ---------------------------------------------------------------------------

/**
 * In-memory `TaskService` backend: `watchTaskList` replays `watchTaskListEvents` (snapshot + any
 * live events) as soon as the stream opens; `watchTask` replays `watchTaskOutput` (when given) for
 * whichever channel the caller asks for; `cancelTask` always succeeds and records every call in
 * `cancelTaskCalls` (for tests asserting cancel was invoked).
 */
export function aTaskServiceBackend(options: {
  watchTaskListEvents: TaskListEvent[];
  watchTaskOutput?: string;
}): InMemoryRpcBackend & { cancelTaskCalls: { taskId: string }[] } {
  const cancelTaskCalls: { taskId: string }[] = [];
  const backend = anInMemoryRpcBackend().implement(TaskService, {
    watchTaskList: async function* () {
      for (const event of options.watchTaskListEvents) yield event;
    },
    watchTask: async function* (req) {
      if (options.watchTaskOutput === undefined) return;
      yield create(TaskOutputEventSchema, {
        channelId: req.channelId,
        data: new TextEncoder().encode(options.watchTaskOutput),
        isReplay: true,
        status: TaskStatusProto.TASK_STATUS_COMPLETED,
      });
    },
    cancelTask: async (req) => {
      cancelTaskCalls.push({ taskId: req.taskId });
      return create(CancelTaskResponseSchema, { ok: true, message: "" });
    },
  });
  return Object.assign(backend, { cancelTaskCalls });
}
