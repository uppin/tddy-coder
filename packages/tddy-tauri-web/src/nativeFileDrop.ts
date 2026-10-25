/**
 * Files dropped onto the host application's window, as a page sees them.
 *
 * The application keeps Tauri's native drag-drop handler on: it is the only thing that knows the
 * dropped files' real paths, and while it is on the webview never sees an HTML5 `dragover` or
 * `drop` at all. So the page learns of a drag from the window — where the pointer is and, on enter
 * and drop, which paths are being carried — and does its own hit-testing against those positions.
 *
 * Reading a dropped file's bytes goes through two host commands that serve **only the paths of
 * the most recent drop** (`packages/tddy-desktop/src-tauri/src/file_drop.rs`).
 */

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";

/** A point in the page's viewport, in CSS pixels — what `document.elementFromPoint` takes. */
export interface ViewportPoint {
  x: number;
  y: number;
}

/** One step of a drag over the window. */
export type NativeDragEvent =
  | { type: "enter"; paths: string[]; point: ViewportPoint }
  | { type: "over"; point: ViewportPoint }
  | { type: "drop"; paths: string[]; point: ViewportPoint }
  | { type: "leave" };

/** What the host says about one dropped path. */
export interface DroppedFileInfo {
  path: string;
  /** The final path segment — the name an upload stores the file under. */
  name: string;
  /** Bytes, for a regular file; 0 for anything else. */
  size: number;
  /** `false` for a folder, which has no bytes to upload. */
  isFile: boolean;
}

/** The window's drag-drop surface, as a page needs it. */
export interface NativeFileDropBridge {
  /** Hear every drag step over the window until the returned function is called. */
  listen(onEvent: (event: NativeDragEvent) => void): Promise<() => void>;
  /** Describe dropped paths, in order. Rejects if any was not part of the most recent drop. */
  describe(paths: string[]): Promise<DroppedFileInfo[]>;
  /** Up to `length` bytes of a dropped file from `offset`; fewer at its end. */
  read(path: string, offset: number, length: number): Promise<Uint8Array>;
}

/** The commands `file_drop.rs` serves, and the event it relays each drag step on. */
export const DESCRIBE_DROPPED_FILES_COMMAND = "tddy_dropped_files_describe";
export const READ_DROPPED_FILE_COMMAND = "tddy_dropped_file_read";
export const NATIVE_DRAG_EVENT = "tddy://native-file-drop";

/**
 * This window's drag-drop surface. Only meaningful inside the host application.
 *
 * Listens to the application's own relay of each drag step rather than to Tauri's
 * `onDragDropEvent`: Tauri types every drag position as physical pixels, but WKWebView and GTK
 * report points, so no single conversion on this side is right on every platform. The host
 * converts with the platform known (`file_drop::drag_step_of`) and sends CSS pixels.
 */
export function thisWindowsNativeFileDrop(): NativeFileDropBridge {
  return {
    listen: (onEvent) =>
      getCurrentWebview().listen<NativeDragEvent>(NATIVE_DRAG_EVENT, (event) =>
        onEvent(event.payload),
      ),
    describe: (paths) => invoke<DroppedFileInfo[]>(DESCRIBE_DROPPED_FILES_COMMAND, { paths }),
    read: async (path, offset, length) =>
      new Uint8Array(
        await invoke<ArrayBuffer>(READ_DROPPED_FILE_COMMAND, {
          path,
          offset,
          length,
        }),
      ),
  };
}
