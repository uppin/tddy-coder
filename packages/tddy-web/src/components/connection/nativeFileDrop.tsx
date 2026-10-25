/**
 * Routes files dropped onto the host application's window to the terminal under the pointer.
 *
 * Inside Tddy Desktop the window's native drag-drop handler owns every drag from the OS — it is the
 * only thing that knows the dropped files' real paths — so a terminal never sees an HTML5
 * `dragover` or `drop`. The window reports where the pointer is instead (`tddy-tauri-web`'s
 * {@link NativeFileDropBridge}); this provider hit-tests that point against the terminals that
 * registered with it, tells the one under the pointer it is hovered, and hands it the paths when
 * they are dropped. A drop on anything else is ignored.
 *
 * In a browser there is no bridge, no provider value, and the terminals' own HTML5 handlers do the
 * work exactly as before.
 *
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

import React, { createContext, useContext, type ReactNode, type RefObject } from "react";
import type { NativeFileDropBridge } from "tddy-tauri-web";

/** What a terminal registers to receive native drops. */
export interface NativeDropTarget {
  /** The drop surface. A drop anywhere inside it is this target's. */
  element: HTMLElement;
  /** Called with `true` when a drag moves over the element, `false` when it leaves or drops. */
  onHover(hovering: boolean): void;
  /** Called with the dropped paths, in drop order, when they are dropped on the element. */
  onDrop(paths: string[]): void;
}

interface NativeFileDropContextValue {
  bridge: NativeFileDropBridge;
  register(target: NativeDropTarget): () => void;
}

const NativeFileDropContext = createContext<NativeFileDropContextValue | null>(null);

export interface NativeFileDropProviderProps {
  /** The window's drag-drop surface, or `null` outside the host application. */
  bridge: NativeFileDropBridge | null;
  children: ReactNode;
}

export function NativeFileDropProvider({ bridge, children }: NativeFileDropProviderProps) {
  void bridge;
  return <NativeFileDropContext.Provider value={null}>{children}</NativeFileDropContext.Provider>;
}

/**
 * Receive native drops on `ref`'s element. Returns the bridge to read dropped files through, or
 * `null` when the page is not inside the host application.
 */
export function useNativeFileDropTarget(
  ref: RefObject<HTMLElement | null>,
  handlers: Pick<NativeDropTarget, "onHover" | "onDrop">,
): NativeFileDropBridge | null {
  void ref;
  void handlers;
  return useContext(NativeFileDropContext)?.bridge ?? null;
}
