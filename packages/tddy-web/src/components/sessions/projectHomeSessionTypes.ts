/**
 * Which session types can run a project's **main** (project-home session).
 *
 * main is read-only, and the daemon can hold only a `claude-cli` agent to that: every other type
 * either has no tool allowlist or cannot run in managed-codebase mode. The main variant of the
 * New-session form shows every type and disables the others with the reason this returns. The
 * daemon refuses them on its own (`EnsureProjectHomeSession`), so this is the explanation, not the
 * guard.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-project-home-from-new-session-form.md
 */

import type { SessionType } from "./createSessionRequest";

/** Why `sessionType` cannot run main, or `null` when it can. */
export function whyMainCannotRun(_sessionType: SessionType): string | null {
  // TODO: implement (docs/dev/1-WIP/2026-10-24-project-home-from-new-session-form.md).
  throw new Error("whyMainCannotRun is not implemented");
}
