import React from "react";
import { terminalKindLabel } from "tddy-session-create-web";

/** One item of a "+" menu group: the terminal kind's label, opening it when chosen. */
export function MenuKindItem({
  kind,
  testId,
  onChoose,
}: {
  kind: string;
  testId: string;
  onChoose: () => void;
}) {
  return (
    <li role="none">
      <button
        type="button"
        role="menuitem"
        data-testid={testId}
        onClick={onChoose}
        className="block w-full px-3 py-2 text-left text-sm hover:bg-muted"
      >
        {terminalKindLabel(kind)}
      </button>
    </li>
  );
}
