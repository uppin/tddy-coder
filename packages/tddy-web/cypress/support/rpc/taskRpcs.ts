/**
 * Cypress intercept helpers for the tasks.TaskService RPC endpoints.
 *
 * Covers both the new WatchTaskList server-streaming endpoint and the existing
 * unary endpoints (CancelTask, WatchTask output).
 *
 * Streaming responses use the Connect protocol envelope format:
 *   [flags:1 byte][length:4 BE bytes][payload]
 *   flags 0x00 = message frame, 0x02 = end-stream frame
 */

import { create, toBinary } from "@bufbuild/protobuf";
import {
  CancelTaskResponseSchema,
  TaskChannelInfoSchema,
  TaskInfoSchema,
  TaskListEventSchema,
  TaskOutputEventSchema,
  TaskStatusProto,
  ChannelKindProto,
  type TaskInfo,
  type TaskListEvent,
} from "../../../src/gen/tasks_pb";
import { toArrayBuffer } from "./protoRpc";

// ---------------------------------------------------------------------------
// Connect protocol framing helpers
// ---------------------------------------------------------------------------

/** Wrap a payload in a Connect message frame (flags=0x00). */
function connectMessageFrame(payload: Uint8Array): Uint8Array {
  const frame = new Uint8Array(5 + payload.length);
  frame[0] = 0x00;
  const view = new DataView(frame.buffer);
  view.setUint32(1, payload.length, false);
  frame.set(payload, 5);
  return frame;
}

/** Produce the end-stream frame (flags=0x02, empty payload). */
function connectEndStreamFrame(): Uint8Array {
  const frame = new Uint8Array(5);
  frame[0] = 0x02;
  return frame;
}

/** Build a complete Connect streaming response body from a list of serialized messages. */
function buildStreamingBody(frames: Uint8Array[]): ArrayBuffer {
  const allFrames = [...frames, connectEndStreamFrame()];
  const total = allFrames.reduce((n, f) => n + f.length, 0);
  const result = new Uint8Array(total);
  let offset = 0;
  for (const f of allFrames) {
    result.set(f, offset);
    offset += f.length;
  }
  return result.buffer;
}

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

// ---------------------------------------------------------------------------
// WatchTaskList streaming intercept
// ---------------------------------------------------------------------------

/** Build a WatchTaskList streaming response body from a list of TaskListEvent objects. */
export function buildWatchTaskListResponse(events: TaskListEvent[]): ArrayBuffer {
  const frames = events.map((e) =>
    connectMessageFrame(toBinary(TaskListEventSchema, e))
  );
  return buildStreamingBody(frames);
}

/** Helper: create a snapshot task_added event. */
export function snapshotTaskAdded(task: TaskInfo): TaskListEvent {
  return create(TaskListEventSchema, {
    isSnapshot: true,
    event: { case: "taskAdded", value: task },
  });
}

/** Helper: create a live task_added event. */
export function liveTaskAdded(task: TaskInfo): TaskListEvent {
  return create(TaskListEventSchema, {
    isSnapshot: false,
    event: { case: "taskAdded", value: task },
  });
}

/** Helper: create a live task_updated event. */
export function liveTaskUpdated(task: TaskInfo): TaskListEvent {
  return create(TaskListEventSchema, {
    isSnapshot: false,
    event: { case: "taskUpdated", value: task },
  });
}

/**
 * Register a cy.intercept for WatchTaskList that returns the given events.
 *
 * @param events  TaskListEvent messages to stream (snapshot + live)
 * @param alias   Cypress alias (default: "watchTaskList")
 */
export function interceptWatchTaskList(events: TaskListEvent[], alias = "watchTaskList"): void {
  const body = buildWatchTaskListResponse(events);
  cy.intercept("POST", "**/rpc/tasks.TaskService/WatchTaskList", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/connect+proto" },
      body,
    });
  }).as(alias);
}

// ---------------------------------------------------------------------------
// WatchTask streaming intercept
// ---------------------------------------------------------------------------

/**
 * Register a cy.intercept for WatchTask that returns the given text as UTF-8 bytes.
 *
 * @param channelId  The channel_id to embed in each event
 * @param text       The text to return as output bytes (split into chunks)
 * @param alias      Cypress alias (default: "watchTask")
 */
export function interceptWatchTask(
  channelId: string,
  text: string,
  alias = "watchTask"
): void {
  const payload = new TextEncoder().encode(text);
  const event = create(TaskOutputEventSchema, {
    channelId,
    data: payload,
    isReplay: true,
    status: TaskStatusProto.TASK_STATUS_COMPLETED,
  });
  const frame = connectMessageFrame(toBinary(TaskOutputEventSchema, event));
  const body = buildStreamingBody([frame]);

  cy.intercept("POST", "**/rpc/tasks.TaskService/WatchTask", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/connect+proto" },
      body,
    });
  }).as(alias);
}

// ---------------------------------------------------------------------------
// CancelTask intercept
// ---------------------------------------------------------------------------

/**
 * Register a cy.intercept for CancelTask that returns ok=true.
 *
 * @param alias  Cypress alias (default: "cancelTask")
 */
export function interceptCancelTask(alias = "cancelTask"): void {
  const body = toArrayBuffer(
    toBinary(CancelTaskResponseSchema, create(CancelTaskResponseSchema, { ok: true, message: "" }))
  );
  cy.intercept("POST", "**/rpc/tasks.TaskService/CancelTask", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body,
    });
  }).as(alias);
}
