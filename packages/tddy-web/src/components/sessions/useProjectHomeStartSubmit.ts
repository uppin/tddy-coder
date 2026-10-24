import { useState } from "react";
import { rpcFailureText } from "../../rpc/rpcFailure";
import type { CreateSessionProjectHome } from "./createSessionPaneProps";
import { buildProjectHomeStartSpec, type CreateSessionFormValues } from "./createSessionRequest";

/** What the form's footer runs: whether submitting is possible, the submit itself and its outcome. */
export interface CreateSessionFooterSubmit {
  submitting: boolean;
  error: string | null;
  isSubmitEnabled: boolean;
  handleSubmit: () => void;
}

/**
 * The submit of the New-session form's **main** variant: **Start main session** hands the form's
 * start spec to `projectHome.onStart` (an `EnsureProjectHomeSession` carrying it) instead of
 * starting a session itself, and a refusal is shown in the form's own error line. Without
 * `projectHome` the form's ordinary submit (`created`) is returned unchanged.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-project-home-from-new-session-form.md
 */
export function useProjectHomeStartSubmit(
  projectHome: CreateSessionProjectHome | undefined,
  form: CreateSessionFormValues,
  created: CreateSessionFooterSubmit,
): CreateSessionFooterSubmit {
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!projectHome) return created;

  const handleSubmit = () => {
    setSubmitting(true);
    setError(null);
    projectHome
      .onStart(buildProjectHomeStartSpec(form))
      .catch((e: unknown) => setError(rpcFailureText(e)))
      .finally(() => setSubmitting(false));
  };

  return {
    submitting,
    error,
    // The project and host are the variant's own; a model is what the operator must still choose.
    isSubmitEnabled: !submitting && form.model !== "",
    handleSubmit,
  };
}
