/**
 * The create-session placement algebra.
 *
 * `Sandbox`, `Managed codebase` and `Sandboxed codebase` each name where a session's agent and its
 * checkout live, and a session has exactly one placement — so choosing any one of them clears the
 * other two. The rule lives here rather than inline in `CreateSessionPane.tsx` so it can be tested
 * exhaustively and the pane grows only by the control and its wiring
 * (docs/dev/todo/2026-08-19-session-creation-agent-catalog-the-rest-of-the-fan-out.md).
 *
 * PRD: docs/ft/daemon/amendments/PRD-2026-09-18-sandboxed-codebase-from-the-web.md
 */

import type { DaemonHost } from "../../lib/participantRole";

/**
 * Which placement the form holds.
 *
 * - `none` — agent and checkout on the host, unconfined. What every session was before any of this.
 * - `sandbox` — the agent is jailed on its own daemon, and the checkout with it.
 * - `managed` — the agent is jailed and the checkout stays on the host, reachable over the tool
 *   proxy the managed codebase installs.
 * - `sandboxedCodebase` — the inverted placement: the *checkout* is jailed and the agent runs
 *   beside it, unconfined, with its native filesystem and shell tools withdrawn.
 */
export type CodebasePlacementChoice = "none" | "sandbox" | "managed" | "sandboxedCodebase";

/**
 * The placement after `toggled`'s checkbox is switched `on` (or off), given the `current` one.
 *
 * Turning a control on makes it *the* placement, replacing whatever was there — that is the whole
 * of the exclusivity rule, and it holds in every direction. Turning one off clears the placement
 * only when it is the one that was chosen: unchecking an already-clear control must not take away
 * a placement the operator did choose.
 */
export function placementAfterToggling(
  current: CodebasePlacementChoice,
  toggled: CodebasePlacementChoice,
  on: boolean,
): CodebasePlacementChoice {
  if (on) return toggled;
  return current === toggled ? "none" : current;
}

/**
 * Why `host` cannot serve the sandboxed-codebase placement, or `null` when it can.
 *
 * The capability is **advertised**, never inferred from a platform string: a host that advertises
 * it serves it, and an older daemon that does not would answer the unrecognised request field by
 * starting an ordinary, unconfined session. That silent downgrade is exactly what the control's
 * disabled state exists to prevent, so absence is unavailability — and the reason names the host,
 * because "the option is missing" teaches an operator nothing about which machine to look at.
 *
 * `confinesFilesystem: false` is **not** a reason to withhold the placement: it is today's Linux
 * cgroups jail, which confines process and network but not writes outside the checkout. What such
 * a jail does not confine is a caveat beside an enabled control (`sandboxedCodebaseCaveat` in
 * `CreateSessionPane`), not a refusal.
 *
 * A `null` host is a form that cannot name the daemon it would run on — the directory has told it
 * nothing — and a capability nobody advertised is one this form may not assume.
 */
export function sandboxedCodebaseUnavailability(host: DaemonHost | null): string | null {
  if (host === null) {
    return "No host is selected, so the sandboxed codebase placement cannot be offered.";
  }
  if (host.sandboxedCodebase) return null;
  return (
    `${host.label || host.instanceId} does not offer the sandboxed codebase placement: ` +
    "it is a daemon that would start an ordinary, unconfined session instead."
  );
}
