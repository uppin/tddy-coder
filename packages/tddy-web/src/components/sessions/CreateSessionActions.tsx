import { Button } from "../ui/button";

/** What cancelling a start in progress needs from the form's submit. */
export interface StartCancel {
  /** The start has named its task and is not already being cancelled. */
  enabled: boolean;
  cancelling: boolean;
  /** The host ended the last start because it was cancelled. */
  cancelled: boolean;
  cancel: () => void;
}

export interface CreateSessionActionsProps {
  /** The daemon's refusal, or `null`. Rendered above the buttons, never swallowed. */
  error: string | null;
  submitting: boolean;
  isSubmitEnabled: boolean;
  onCancel: () => void;
  /**
   * Cancelling a start in progress. When given, Cancel during a start asks the host to cancel the
   * start's task (instead of closing the pane); when the host ended the start because of it,
   * `cancelled` shows the notice. Absent: Cancel is disabled while submitting.
   */
  startCancel?: StartCancel;
  handleSubmit: () => void;
  /** The submit button's text; defaults to **Create session**. */
  submitLabel?: string;
}

/**
 * The form's footer: the daemon's refusal, and the Cancel / Create buttons. Presentational — what
 * makes Create submittable, and what submitting does, are `CreateSessionPane`'s.
 */
export function CreateSessionActions({
  error,
  submitting,
  isSubmitEnabled,
  onCancel,
  startCancel,
  handleSubmit,
  submitLabel = "Create session",
}: CreateSessionActionsProps) {
  return (
    <>
      {error !== null && (
        <p data-testid="create-session-error" className="text-sm text-destructive">
          {error}
        </p>
      )}

      {startCancel?.cancelled && !submitting && (
        <p data-testid="create-session-start-cancelled" className="text-sm text-muted-foreground">
          Start cancelled
        </p>
      )}

      {/* Actions */}
      <div className="flex gap-2 pt-2">
        <Button
          type="button"
          data-testid="create-session-cancel-btn"
          variant="outline"
          onClick={submitting && startCancel ? startCancel.cancel : onCancel}
          disabled={submitting && !(startCancel && startCancel.enabled)}
        >
          {startCancel?.cancelling ? "Cancelling…" : "Cancel"}
        </Button>
        <Button
          type="button"
          data-testid="create-session-submit-btn"
          disabled={!isSubmitEnabled}
          onClick={handleSubmit}
        >
          {submitLabel}
        </Button>
      </div>
    </>
  );
}
