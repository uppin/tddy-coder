/**
 * A stand-in for the host application window's drag-drop surface (`tddy-tauri-web`'s
 * `NativeFileDropBridge`), so a component test can drag files "from Finder" onto a terminal.
 *
 * The real window reports positions and paths and serves the bytes of the most recent drop; this
 * double does the same over an in-memory file table, and records what the page asked of it.
 *
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

import type { DroppedFileInfo, NativeDragEvent, NativeFileDropBridge } from "tddy-tauri-web";

/** A dropped path's contents: its bytes, or `"folder"`. */
export type DroppedEntry = Uint8Array | "folder";

export interface NativeFileDropDouble {
  bridge: NativeFileDropBridge;
  /** Deliver one drag step to whatever the page registered. */
  emit(event: NativeDragEvent): void;
  /** Paths the page asked the host to describe, per call. */
  described: string[][];
  /** Every `read` the page made. */
  reads: Array<{ path: string; offset: number; length: number }>;
}

export function aNativeFileDropDouble(
  files: Record<string, DroppedEntry> = {},
): NativeFileDropDouble {
  const listeners = new Set<(event: NativeDragEvent) => void>();
  const described: string[][] = [];
  const reads: Array<{ path: string; offset: number; length: number }> = [];

  const entry = (path: string): DroppedEntry => {
    const found = files[path];
    if (found === undefined) throw new Error(`${path} was not dropped`);
    return found;
  };

  const bridge: NativeFileDropBridge = {
    listen: async (onEvent) => {
      listeners.add(onEvent);
      return () => listeners.delete(onEvent);
    },
    describe: async (paths) => {
      described.push(paths);
      return paths.map((path): DroppedFileInfo => {
        const found = entry(path);
        return {
          path,
          name: path.split("/").pop() ?? path,
          size: found === "folder" ? 0 : found.length,
          isFile: found !== "folder",
        };
      });
    },
    read: async (path, offset, length) => {
      reads.push({ path, offset, length });
      const found = entry(path);
      if (found === "folder") throw new Error(`${path} is a folder`);
      return found.slice(offset, offset + length);
    },
  };

  return {
    bridge,
    emit: (event) => listeners.forEach((listener) => listener(event)),
    described,
    reads,
  };
}

/** The centre of the first element matching `selector`, in viewport CSS pixels. */
export function centreOf(selector: string): Cypress.Chainable<{ x: number; y: number }> {
  return cy.get(selector).then(($el) => {
    const rect = $el[0].getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  });
}
