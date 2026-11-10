import { useEffect, useState } from "react";

const KEYBOARD_THRESHOLD_PX = 150;

export interface VisualViewportState {
  height: number;
  offsetTop: number;
  isKeyboardOpen: boolean;
}

/**
 * Tracks window.visualViewport for mobile keyboard-aware layout.
 * When the virtual keyboard opens, visualViewport.height shrinks.
 */
export function useVisualViewport(): VisualViewportState {
  const vv =
    typeof window !== "undefined" ? window.visualViewport : null;
  const [state, setState] = useState<VisualViewportState>(() => {
    if (!vv) {
      return {
        height: typeof window !== "undefined" ? window.innerHeight : 0,
        offsetTop: 0,
        isKeyboardOpen: false,
      };
    }
    const height = vv.height;
    const layoutHeight = window.innerHeight;
    const isKeyboardOpen = layoutHeight - height > KEYBOARD_THRESHOLD_PX;
    return {
      height,
      offsetTop: vv.offsetTop,
      isKeyboardOpen,
    };
  });

  useEffect(() => {
    if (!vv) return;
    const update = () => {
      const height = vv.height;
      const layoutHeight = window.innerHeight;
      const isKeyboardOpen = layoutHeight - height > KEYBOARD_THRESHOLD_PX;
      setState({
        height,
        offsetTop: vv.offsetTop,
        isKeyboardOpen,
      });
    };
    vv.addEventListener("resize", update);
    vv.addEventListener("scroll", update);
    // Android Chrome may fire window.resize instead of (or in addition to) visualViewport.resize when keyboard closes
    window.addEventListener("resize", update);
    return () => {
      vv.removeEventListener("resize", update);
      vv.removeEventListener("scroll", update);
      window.removeEventListener("resize", update);
    };
  }, [vv]);

  return state;
}
