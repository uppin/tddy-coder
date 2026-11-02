/**
 * A short message floating over the content it is about, with at most one action — "This session is
 * not running · Resume". `role="status"` so assistive tech reads it without moving focus; an action
 * error renders inside it with `role="alert"`. Positioned absolutely: the caller's container is the
 * containing block, as for `TerminalControlOverlay`.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-02-dormant-agent-tab.md (`#session-tabs 6/7`)
 */

interface ToastAction {
  label: string
  onClick: () => void
  /** While true the action shows it is working and cannot be pressed again. */
  busy?: boolean
}

interface ToastProps {
  message: string
  action?: ToastAction
  /** Why the action last failed; rendered with `role="alert"`. */
  error?: string | null
  /** The toast's test id; its action is `<testId>-action`, its error `<testId>-error`. */
  testId: string
}

function Toast(_props: ToastProps) {
  // TODO(dormant-agent-tab): implement
  return null
}

export { Toast }
export type { ToastAction, ToastProps }
