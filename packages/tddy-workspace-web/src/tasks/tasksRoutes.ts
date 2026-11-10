/** The Tasks screen's URL grammar — `/tasks` and `/tasks/:taskId`. Pure string rules. */

import { decodeSegment, singleSegmentAfter } from "tddy-client-web";

/** Canonical path for the Tasks management screen. */
export const TASKS_ROUTE = "/tasks";

/**
 * Returns true for `/tasks` (the drawer root) and `/tasks/:taskId` (deep links).
 * Does NOT match `/tasks-archive` or other paths that merely start with `/tasks`.
 */
export function isTasksPath(pathname: string): boolean {
  return pathname === TASKS_ROUTE || singleSegmentAfter(pathname, TASKS_ROUTE) !== null;
}

/** Builds a `/tasks/:taskId` deep-link path for the given task id. */
export function tasksPathForTask(taskId: string): string {
  return `${TASKS_ROUTE}/${encodeURIComponent(taskId)}`;
}

/**
 * Extracts the task id from a `/tasks/:taskId` pathname.
 * Returns `null` for `/tasks` (no segment) or non-matching paths.
 */
export function parseTaskId(pathname: string): string | null {
  return decodeSegment(singleSegmentAfter(pathname, TASKS_ROUTE));
}
