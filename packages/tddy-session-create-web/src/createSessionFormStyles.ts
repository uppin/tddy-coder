/**
 * The input and label classes the new-session form's controls share.
 *
 * Module-scope so a control extracted into its own file — the **Agent** select, and whatever follows
 * it out of `CreateSessionPane` — keeps looking like the controls it sits between, without either
 * re-spelling the class list or importing styling from its parent.
 */

export const inputClass =
  "w-full rounded-md border border-input bg-background px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

export const labelClass = "block text-sm mb-1 text-muted-foreground";
