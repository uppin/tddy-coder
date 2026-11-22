import React from "react";
import { MenuKindItem } from "./MenuKindItem";

/** One group of {@link WrappedKindGroups}: its aria-label and the prefix of its items' test ids. */
export interface KindGroup {
  /** The group's `aria-label` — "In sandbox", "On host", "On target", "On this host". */
  label: string;
  /** Each item's `data-testid` is this prefix, `-`, and the item's kind. */
  testIdPrefix: string;
}

export interface WrappedKindGroupsProps {
  /** The kinds both groups offer, in order: bash, then every agent kind. */
  kinds: readonly string[];
  /** The first group, the default: each of its kinds opens wrapped in `wrapper`. It carries the
   *  menu's `sessions-terminal-tab-new-menu` test id. */
  wrapped: KindGroup & { wrapper: string };
  /** The second group: each of its kinds opens unwrapped. */
  unwrapped: KindGroup;
  /** A kind was chosen: with `[wrapped.wrapper]` from the first group, with no wrappers from the
   *  second. */
  onChoose: (kind: string, wrappers?: readonly string[]) => void;
}

/**
 * The "+" menu's two groups for a session whose tabs can open in one place or another — "In sandbox"
 * and "On host" for a sandboxed session, "On target" and "On this host" for a target session: the
 * same kinds twice, the first group's wrapped, the second's not (`#tab-hardening` 2/4).
 */
export function WrappedKindGroups({ kinds, wrapped, unwrapped, onChoose }: WrappedKindGroupsProps) {
  return (
    <>
      <ul role="group" aria-label={wrapped.label} data-testid="sessions-terminal-tab-new-menu">
        {kinds.map((kind) => (
          <MenuKindItem
            key={kind}
            kind={kind}
            testId={`${wrapped.testIdPrefix}-${kind}`}
            onChoose={() => onChoose(kind, [wrapped.wrapper])}
          />
        ))}
      </ul>
      <ul role="group" aria-label={unwrapped.label} className="mt-1 border-t border-border pt-1">
        {kinds.map((kind) => (
          <MenuKindItem
            key={kind}
            kind={kind}
            testId={`${unwrapped.testIdPrefix}-${kind}`}
            onChoose={() => onChoose(kind)}
          />
        ))}
      </ul>
    </>
  );
}
