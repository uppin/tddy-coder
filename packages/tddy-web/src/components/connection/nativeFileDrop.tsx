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

import React, {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  type ReactNode,
  type RefObject,
} from "react";
import type { DroppedFileInfo, NativeFileDropBridge, ViewportPoint } from "tddy-tauri-web";
import { UPLOAD_CHUNK_SIZE } from "../../lib/fileUploadChunks";
import type { UploadSource } from "../../hooks/useSessionFileUpload";

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
  const targetsRef = useRef(new Map<HTMLElement, NativeDropTarget>());
  const hoveredRef = useRef<NativeDropTarget | null>(null);

  useEffect(() => {
    if (!bridge) return;
    const targets = targetsRef.current;
    const hover = (next: NativeDropTarget | null) => {
      if (hoveredRef.current === next) return;
      hoveredRef.current?.onHover(false);
      hoveredRef.current = next;
      next?.onHover(true);
    };
    // The innermost registered element at the point. `elementFromPoint` skips what is hidden or
    // `pointer-events: none`, so a terminal in a background pane is never the one hit.
    const targetAt = (point: ViewportPoint): NativeDropTarget | null => {
      for (let el = document.elementFromPoint(point.x, point.y); el; el = el.parentElement) {
        const target = el instanceof HTMLElement ? targets.get(el) : undefined;
        if (target) return target;
      }
      return null;
    };
    // A drag that carries no paths is not a file from the OS — it started inside the page (a
    // Files-tab entry), whose own HTML5 handlers serve it — so this route has nothing to deliver.
    let carriesPaths = false;

    let disposed = false;
    let unlisten: (() => void) | null = null;
    void bridge
      .listen((event) => {
        switch (event.type) {
          case "enter":
            carriesPaths = event.paths.length > 0;
            hover(carriesPaths ? targetAt(event.point) : null);
            break;
          case "over":
            hover(carriesPaths ? targetAt(event.point) : null);
            break;
          case "drop": {
            const target = event.paths.length > 0 ? targetAt(event.point) : null;
            hover(null);
            carriesPaths = false;
            target?.onDrop(event.paths);
            break;
          }
          case "leave":
            hover(null);
            carriesPaths = false;
            break;
        }
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      // Files dropped on the window then reach no terminal, but the dashboard keeps running — a
      // failed subscription must not take down every screen with it.
      .catch((error: unknown) => {
        console.error(
          "[NativeFileDropProvider] the window's drag events could not be listened to; files dropped on it will not reach a terminal",
          error,
        );
      });
    return () => {
      disposed = true;
      unlisten?.();
      hover(null);
    };
  }, [bridge]);

  const value = useMemo<NativeFileDropContextValue | null>(
    () =>
      bridge && {
        bridge,
        register: (target) => {
          targetsRef.current.set(target.element, target);
          return () => {
            if (targetsRef.current.get(target.element) === target) {
              targetsRef.current.delete(target.element);
            }
            if (hoveredRef.current === target) hoveredRef.current = null;
          };
        },
      },
    [bridge],
  );

  return <NativeFileDropContext.Provider value={value}>{children}</NativeFileDropContext.Provider>;
}

/**
 * Receive native drops on `ref`'s element. Returns the bridge to read dropped files through, or
 * `null` when the page is not inside the host application.
 */
export function useNativeFileDropTarget(
  ref: RefObject<HTMLElement | null>,
  handlers: Pick<NativeDropTarget, "onHover" | "onDrop">,
): NativeFileDropBridge | null {
  const context = useContext(NativeFileDropContext);
  const handlersRef = useRef(handlers);
  handlersRef.current = handlers;

  useEffect(() => {
    const element = ref.current;
    if (!context || !element) return;
    return context.register({
      element,
      onHover: (hovering) => handlersRef.current.onHover(hovering),
      onDrop: (paths) => handlersRef.current.onDrop(paths),
    });
  }, [context, ref]);

  return context?.bridge ?? null;
}

/** A dropped file as an upload, its bytes read from this machine in upload-sized slices. */
export function droppedFileUploadSource(
  bridge: NativeFileDropBridge,
  file: DroppedFileInfo,
): UploadSource {
  return {
    name: file.name,
    size: file.size,
    // A zero-byte file is still one (empty) chunk, so the final chunk fires and a path comes back.
    chunkCount: Math.max(1, Math.ceil(file.size / UPLOAD_CHUNK_SIZE)),
    readChunk: (index) => bridge.read(file.path, index * UPLOAD_CHUNK_SIZE, UPLOAD_CHUNK_SIZE),
  };
}
