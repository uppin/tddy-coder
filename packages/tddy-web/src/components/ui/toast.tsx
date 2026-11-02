import { Loader2 } from "lucide-react"
import { Button } from "./button"
import { InlineError } from "./inline-error"

/**
 * A short message floating over the content it is about, with at most one action — "This session is
 * not running · Resume". `role="status"` so assistive tech reads it without moving focus; an action
 * error renders inside it with `role="alert"`. Positioned absolutely: the caller's container is the
 * containing block, as for `TerminalControlOverlay`.
 *
 * PRD: docs/ft/web/session-terminal-tabs.md (`#session-tabs 6/7`)
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

function Toast({ message, action, error = null, testId }: ToastProps) {
  return (
    <div
      data-testid={testId}
      role="status"
      className="pointer-events-auto absolute left-1/2 top-3 z-10 flex max-w-[95%] -translate-x-1/2 flex-col items-center"
    >
      <div className="flex items-center gap-3 rounded-md border border-border bg-background/95 py-1.5 pl-3 pr-1.5 shadow-lg">
        <span className="text-xs text-muted-foreground">{message}</span>
        {action && (
          <Button
            size="sm"
            data-testid={`${testId}-action`}
            disabled={action.busy === true}
            onClick={() => action.onClick()}
          >
            {action.busy && <Loader2 className="animate-spin" aria-hidden />}
            {action.label}
          </Button>
        )}
      </div>
      <InlineError testId={`${testId}-error`} message={error} className="mt-2 max-w-md text-center" />
    </div>
  )
}

export { Toast }
export type { ToastAction, ToastProps }
