/**
 * A provider account's models on its Accounts-screen row, asked of the account's endpoint only when
 * a person expands the row.
 */

import { useState } from "react";
import { Button } from "../ui/button";

type ModelsState =
  | { kind: "closed" }
  | { kind: "loading" }
  | { kind: "listed"; modelIds: string[] }
  | { kind: "error"; reason: string };

/** A provider account's models, asked of its endpoint only when a person expands the row. */
export function ProviderModels({
  rowId,
  accountId,
  onListModels,
}: {
  rowId: string;
  accountId: string;
  onListModels: (accountId: string) => Promise<string[]>;
}) {
  const [state, setState] = useState<ModelsState>({ kind: "closed" });
  const toggle = () => {
    if (state.kind !== "closed") {
      setState({ kind: "closed" });
      return;
    }
    setState({ kind: "loading" });
    onListModels(accountId)
      .then((modelIds) => setState({ kind: "listed", modelIds }))
      .catch((e: unknown) =>
        setState({ kind: "error", reason: e instanceof Error ? e.message : String(e) }),
      );
  };
  return (
    <span className="flex flex-col gap-1">
      <Button size="sm" variant="ghost" data-testid={`${rowId}-models-toggle`} onClick={toggle}>
        {state.kind === "closed" ? "Models" : "Hide models"}
      </Button>
      {state.kind === "loading" ? <span className="text-xs">Asking the provider…</span> : null}
      {state.kind === "error" ? (
        <span data-testid={`${rowId}-models-error`} className="text-xs text-destructive">
          {state.reason}
        </span>
      ) : null}
      {state.kind === "listed" ? (
        <ul data-testid={`${rowId}-models`} className="text-xs font-mono">
          {state.modelIds.map((id) => (
            <li key={id}>{id}</li>
          ))}
        </ul>
      ) : null}
    </span>
  );
}
