import { describe, expect, it } from "bun:test";
import { TASKS_ROUTE, isTasksPath, parseTaskId, tasksPathForTask } from "./tasksRoutes";

describe("tasks routes", () => {
  it("builds the /tasks/:taskId deep-link path", () => {
    // When
    const result = tasksPathForTask("task-9");
    // Then
    expect(result).toBe("/tasks/task-9");
  });

  it("URL-encodes a task id containing a reserved character", () => {
    // When
    const result = tasksPathForTask("task/9");
    // Then
    expect(result).toBe("/tasks/task%2F9");
  });

  it("extracts the task id from a /tasks/:taskId pathname", () => {
    // When
    const result = parseTaskId("/tasks/task-9");
    // Then
    expect(result).toBe("task-9");
  });

  it("returns null for /tasks (no task id segment)", () => {
    // When
    const result = parseTaskId(TASKS_ROUTE);
    // Then
    expect(result).toBeNull();
  });

  it("isTasksPath matches /tasks/:taskId deep links", () => {
    // When
    const result = isTasksPath("/tasks/task-9");
    // Then
    expect(result).toBe(true);
  });

  it("isTasksPath does not match a path that merely starts with /tasks", () => {
    // When
    const result = isTasksPath("/tasks-archive");
    // Then
    expect(result).toBe(false);
  });
});
