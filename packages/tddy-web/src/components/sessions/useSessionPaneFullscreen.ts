import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import {
  exitDocumentFullscreen,
  isTargetInActiveFullscreen,
  requestElementFullscreen,
} from "../../lib/browserFullscreen";

export interface SessionPaneFullscreen {
  /** The pane stack — the Fullscreen API target. */
  readonly paneStackRef: RefObject<HTMLDivElement | null>;
  /** This stack IS or CONTAINS the fullscreen element. */
  readonly fullscreenActive: boolean;
  /** This stack IS the fullscreen element. */
  readonly fullscreenOwned: boolean;
  /** Enter full screen on the pane stack, or leave it. */
  readonly toggleFullscreen: () => void;
}

/**
 * A session runtime's full screen — the pane stack (not an individual pane) is the Fullscreen API
 * target. Only one pane is ever visible, so fullscreening the stack shows exactly the active pane,
 * and it keeps the terminal-control mutex and connection overlays — both siblings of the panes — on
 * screen. Taking a single pane instead would drop the "Claim terminal" CTA behind the fullscreen
 * layer and leave a session whose control another screen holds looking interactive while swallowing
 * every key. The tab strip is deliberately left behind: full screen is the whole viewport for one
 * terminal.
 *
 * `focused` is whether this runtime is the visible one; a hidden runtime that owns full screen
 * leaves it.
 */
export function useSessionPaneFullscreen(focused: boolean): SessionPaneFullscreen {
  const paneStackRef = useRef<HTMLDivElement>(null);
  // `active` is containment-based (this stack IS or CONTAINS the fullscreen element) so a parent
  // runtime whose nested child conversation went fullscreen still offers the exit. `owned` is exact,
  // and gates the floating exit control — otherwise a fullscreen child pane would draw its own exit
  // button and its parent's on top of each other.
  const [fullscreenActive, setFullscreenActive] = useState(false);
  const [fullscreenOwned, setFullscreenOwned] = useState(false);

  useEffect(() => {
    const sync = () => {
      const target = paneStackRef.current;
      setFullscreenActive(isTargetInActiveFullscreen(target));
      setFullscreenOwned(target !== null && document.fullscreenElement === target);
    };
    sync();
    document.addEventListener("fullscreenchange", sync);
    document.addEventListener("webkitfullscreenchange", sync as EventListener);
    return () => {
      document.removeEventListener("fullscreenchange", sync);
      document.removeEventListener("webkitfullscreenchange", sync as EventListener);
    };
  }, []);

  // Selecting another session (or another tab, for a child runtime) hides this runtime behind
  // `display:none`. Browsers generally drop out of fullscreen when an ancestor is hidden, but not
  // uniformly — and a top-layer element left over a hidden ancestor is a black screen the operator
  // cannot navigate out of. Exit deliberately instead of relying on that, but only from the runtime
  // that actually owns fullscreen: `exitDocumentFullscreen` is document-global.
  useEffect(() => {
    if (!focused && fullscreenOwned) void exitDocumentFullscreen().catch(() => undefined);
  }, [focused, fullscreenOwned]);

  const toggleFullscreen = useCallback(() => {
    const target = paneStackRef.current;
    if (isTargetInActiveFullscreen(target)) {
      void exitDocumentFullscreen().catch(() => undefined);
      return;
    }
    // The request rejects when the browser refuses (an iframe without `allowfullscreen`, a gesture
    // the UA does not count as user-activated). Nothing to recover — the pane stays inline.
    void requestElementFullscreen(target).catch(() => undefined);
  }, []);

  return { paneStackRef, fullscreenActive, fullscreenOwned, toggleFullscreen };
}
