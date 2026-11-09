import { useState } from "react";
import type { Dispatch, SetStateAction } from "react";

/**
 * The two tool sets an assistant is composed from, held together because they are only ever
 * meaningful as a pair.
 *
 * **Tools** is the assistant's own loop — what it may call while it works. **Replaces** is what it
 * takes over from the *main* agent: the tools a session that attaches it stops being able to call
 * itself. Neither is derivable from the other, and both dialogs offer them the same way, so the
 * pair and the way it is sent live here rather than being written out twice.
 */
export interface AssistantToolSets {
  /** What the assistant may call in its own loop, as ticked. */
  readonly tools: readonly string[];
  /** The main-agent tools it takes over, as ticked. */
  readonly replaces: readonly string[];
  toggleTool: (toolName: string, checked: boolean) => void;
  toggleReplaced: (toolName: string, checked: boolean) => void;
  /**
   * Both sets in the daemon's catalog order, which is what gets sent: what is stored then does not
   * depend on the order the operator happened to tick the boxes in.
   */
  inCatalogOrder: (catalogTools: readonly { name: string }[]) => {
    tools: string[];
    replaces: string[];
  };
}

export function useAssistantToolSets(
  initialTools: readonly string[],
  initialReplaces: readonly string[],
): AssistantToolSets {
  const [tools, setTools] = useState<string[]>([...initialTools]);
  const [replaces, setReplaces] = useState<string[]>([...initialReplaces]);

  const toggle =
    (set: Dispatch<SetStateAction<string[]>>) =>
    (toolName: string, checked: boolean) =>
      set((current) =>
        checked ? [...current, toolName] : current.filter((t) => t !== toolName),
      );

  return {
    tools,
    replaces,
    toggleTool: toggle(setTools),
    toggleReplaced: toggle(setReplaces),
    inCatalogOrder: (catalogTools) => {
      const order = catalogTools.map((t) => t.name);
      return {
        tools: order.filter((name) => tools.includes(name)),
        replaces: order.filter((name) => replaces.includes(name)),
      };
    },
  };
}
