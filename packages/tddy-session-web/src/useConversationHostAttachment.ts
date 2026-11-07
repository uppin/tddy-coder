import { type RefObject, useLayoutEffect } from "react";

/**
 * Put `host` inside the pane stack while `hosts` holds. A strip that is not the one showing its
 * session passes `hosts = false` and never takes the element; one that unmounts only removes it
 * when it is still the parent, so a successor that already adopted it is not undone.
 */
export function useConversationHostAttachment(
  paneStackRef: RefObject<HTMLElement | null>,
  host: HTMLElement | undefined,
  hosts: boolean,
): void {
  useLayoutEffect(() => {
    const stack = paneStackRef.current;
    if (!hosts || host === undefined || stack === null) return;
    stack.appendChild(host);
    return () => {
      if (host.parentNode === stack) stack.removeChild(host);
    };
  }, [paneStackRef, host, hosts]);
}
