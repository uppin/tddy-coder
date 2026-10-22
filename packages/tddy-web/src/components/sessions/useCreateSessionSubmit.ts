import { useState } from "react";
import { flushSync } from "react-dom";
import type { BranchConflict } from "../../gen/session_pb";
import {
  startSessionOverridesFor,
  type BranchConflictResolution,
  type BranchFieldOverrides,
} from "../../lib/branchConflict";
import type {
  SessionAttachmentInit,
  StartSessionRequestInit,
  useSessionAttachments,
} from "../../hooks/useSessionAttachments";
import type { CreateSessionPaneProps } from "./createSessionPaneProps";
import { buildStartSessionRequest, type CreateSessionFormValues } from "./createSessionRequest";

type SessionAttachments = ReturnType<typeof useSessionAttachments>;

export interface UseCreateSessionSubmitArgs {
  /** The form's current values, as `buildStartSessionRequest` reads them. */
  form: CreateSessionFormValues;
  /** Why an attach row would be refused, or `null` when every row is acceptable. */
  attachmentProblem: SessionAttachments["problem"];
  resetAttachmentProgress: SessionAttachments["resetProgress"];
  stageAttachments: SessionAttachments["stageAttachments"];
  startSessionStreamed: SessionAttachments["startSessionStreamed"];
  onCreated: CreateSessionPaneProps["onCreated"];
}

/**
 * Creating the session: whether Create is enabled, the submitting/error state, the streamed start, and the branch-conflict
 * prompt a refused start opens. Lifted out of `CreateSessionPane.tsx` unchanged — the handlers are
 * rebuilt on every render over that render's form values, exactly as they were inline.
 */
export function useCreateSessionSubmit({
  form,
  attachmentProblem,
  resetAttachmentProgress,
  stageAttachments,
  startSessionStreamed,
  onCreated,
}: UseCreateSessionSubmitArgs) {
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Set when the daemon refused a creation because another session already owns the requested branch.
  // The form stays mounted behind the prompt, so cancelling returns to it with its values intact.
  const [branchConflict, setBranchConflict] = useState<BranchConflict | null>(null);

  const { sessionType, projectId, selectedAgentId, toolPath, model } = form;
  const isSubmitEnabled = (() => {
    if (submitting) return false;
    // An attachment the daemon would refuse (duplicate or unsafe basename) fails the whole creation,
    // so it is refused in the form instead.
    if (attachmentProblem !== null) return false;
    // A model is always required and comes from the daemon-advertised catalog; a failed/loading
    // probe leaves `model` empty, which disables Create (no fallback).
    if (sessionType === "tool") {
      return Boolean(projectId && selectedAgentId && toolPath && model);
    }
    return Boolean(projectId && model);
  })();

  /** Build one `StartSession` request for the current form state. */
  const startSessionRequest = (
    branchOverrides: BranchFieldOverrides | null,
    requestAttachments: SessionAttachmentInit[],
  ): StartSessionRequestInit => buildStartSessionRequest(form, branchOverrides, requestAttachments);

  const submitCreation = async (branchOverrides: BranchFieldOverrides | null) => {
    // Use flushSync to commit the submitting state synchronously before the async fetch starts.
    // This ensures the Create button is visibly disabled in the very next render cycle, even
    // if the network response arrives quickly (e.g. in tests with a fast stub).
    flushSync(() => {
      setSubmitting(true);
      setError(null);
      resetAttachmentProgress();
    });
    try {
      // Uploads only what is not already on the staging host, so answering a branch-conflict prompt
      // re-runs the creation without re-sending bytes that already arrived.
      const requestAttachments: SessionAttachmentInit[] = await stageAttachments();
      const request = startSessionRequest(branchOverrides, requestAttachments);
      // Always streamed: the host reports the step it is in (worktree, agent) as well as the
      // per-attachment progress, and ends the stream with the one result that creates the session.
      const res = await startSessionStreamed(request);
      if (res === null) {
        // The form unmounted while the host was still working; it owns no navigation any more.
        return;
      }
      if (res.branchConflict) {
        // Another session owns the branch and nothing was created — ask the operator how to proceed
        // instead of navigating to a session that does not exist.
        setBranchConflict(res.branchConflict);
        return;
      }
      onCreated(res.sessionId);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setSubmitting(false);
    }
  };

  const handleSubmit = () => submitCreation(null);

  /**
   * Apply the operator's answer to the branch-conflict prompt. "Switch" submits nothing: it hands the
   * owning session's id to `onCreated`, which is what selects and attaches a session. The other two
   * choices re-run creation with the branch fields that choice implies.
   */
  const resolveBranchConflict = (
    conflict: BranchConflict,
    resolution: BranchConflictResolution,
  ) => {
    const branchOverrides = startSessionOverridesFor(resolution, conflict);
    setBranchConflict(null);
    if (branchOverrides === null) {
      // `owner` is only optional because proto3 message fields always are in the generated types —
      // a reported conflict always names the session that holds the branch.
      onCreated(conflict.owner?.sessionId ?? "");
      return;
    }
    void submitCreation(branchOverrides);
  };

  return {
    submitting,
    error,
    branchConflict,
    setBranchConflict,
    isSubmitEnabled,
    handleSubmit,
    resolveBranchConflict,
  };
}
