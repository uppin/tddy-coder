import { Button } from "../ui/button";

export interface CreateSessionActionsProps {
  /** The daemon's refusal, or `null`. Rendered above the buttons, never swallowed. */
  error: string | null;
  submitting: boolean;
  isSubmitEnabled: boolean;
  onCancel: () => void;
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

      {/* Actions */}
      <div className="flex gap-2 pt-2">
        <Button
          type="button"
          data-testid="create-session-cancel-btn"
          variant="outline"
          onClick={onCancel}
          disabled={submitting}
        >
          Cancel
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
