// Pure helpers over `WorkQueueView`'s per-item mutation state (error maps and in-flight sets).

export function withoutNode(errors: Record<string, string>, itemId: string): Record<string, string> {
  if (!(itemId in errors)) return errors;
  const next = { ...errors };
  delete next[itemId];
  return next;
}

export function withoutNodeInFlight(inFlight: ReadonlySet<string>, itemId: string): ReadonlySet<string> {
  const next = new Set(inFlight);
  next.delete(itemId);
  return next;
}

export function failureReason(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function unpushedPullReason(baseBranch: string, branch: string, pushError: string): string {
  return `Pulled ${baseBranch} into ${branch} locally, but the push failed — ${branch} on the remote (and its PR) does not have it yet: ${pushError}`;
}
