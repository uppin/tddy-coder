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
export function whyMainCannotRun(sessionType: SessionType): string | null {
  switch (sessionType) {
    case "claude-cli":
      return null;
    case "cursor-cli":
      // docs/dev/todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md
      return "Cursor CLI has no tool allowlist, so it could still use its own file tools and main would not stay read-only.";
    case "tool":
      return cannotRunManaged("Tool sessions");
    case "opencode-cli":
      return cannotRunManaged("OpenCode");
    case "muse-cli":
      return cannotRunManaged("Muse Code");
  }
}

function cannotRunManaged(label: string): string {
  return `${label} cannot run in managed-codebase mode, so main would not stay read-only.`;
}
