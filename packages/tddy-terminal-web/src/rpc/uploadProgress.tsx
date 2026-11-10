/**
 * Upload-progress store shared between the terminal's drop handler and the Host Stats Footer.
 *
 * The drop handler drives the store (start a drop, advance bytes as chunks upload, mark a failed
 * file, finish the drop); the footer's `UploadProgressIndicator` reads the snapshot. The store's
 * accounting is synchronous and framework-free; the `UploadProgressProvider` owns the visible
 * auto-hide (clearing a lingering error after a drop finishes).
 *
 * Changeset: `terminal-file-drop-upload`
 * PRD: docs/ft/web/host-stats-footer.md § Upload progress
 */

import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useSyncExternalStore,
} from "react";

/** How long a per-file failure lingers in the footer after a drop finishes. */
const ERROR_AUTO_HIDE_MS = 6000;

export interface UploadProgressSnapshot {
  /** True while a drop is in flight. */
  active: boolean;
  /** Number of files in the current drop. */
  fileCount: number;
  /** Floored aggregate percent (0..100). */
  percent: number;
  /** A transient per-file failure message (naming the skipped file), or `null`. */
  error: string | null;
}

/**
 * Synchronous accounting for one drop's aggregate upload progress. Subscribers are notified on
 * every transition; `snapshot()` returns a stable reference between transitions so it is safe for
 * `useSyncExternalStore`.
 */
export class UploadProgressStore {
  private active = false;
  private fileCount = 0;
  private totalBytes = 0;
  private uploadedBytes = 0;
  private error: string | null = null;
  private errorTimer: ReturnType<typeof setTimeout> | null = null;
  private listeners = new Set<() => void>();
  private snap: UploadProgressSnapshot = {
    active: false,
    fileCount: 0,
    percent: 0,
    error: null,
  };

  snapshot(): UploadProgressSnapshot {
    return this.snap;
  }

  subscribe(cb: () => void): () => void {
    this.listeners.add(cb);
    return () => {
      this.listeners.delete(cb);
    };
  }

  /** Begins a drop of `fileCount` files totalling `totalBytes`, clearing any prior error. */
  startDrop(fileCount: number, totalBytes: number): void {
    // Cancel a prior drop's pending auto-hide so it can't wipe this drop's error early.
    this.cancelErrorClear();
    this.active = true;
    this.fileCount = fileCount;
    this.totalBytes = totalBytes;
    this.uploadedBytes = 0;
    this.error = null;
    this.emit();
  }

  /** Records `bytes` more uploaded, advancing the aggregate percent. */
  advance(bytes: number): void {
    this.uploadedBytes += bytes;
    this.emit();
  }

  /** Records that `fileName` failed to upload and was skipped. */
  failFile(fileName: string): void {
    this.error = `Upload failed: ${fileName}`;
    this.emit();
  }

  /**
   * Ends the drop; a recorded error is preserved for the footer to surface, then auto-hidden after
   * {@link ERROR_AUTO_HIDE_MS}.
   */
  finishDrop(): void {
    this.active = false;
    this.emit();
    if (this.error !== null) {
      this.scheduleErrorClear();
    }
  }

  /** Clears a lingering error (used by the auto-hide). */
  clearError(): void {
    this.cancelErrorClear();
    if (this.error === null) return;
    this.error = null;
    this.emit();
  }

  /** Cancels any pending auto-hide timer (call on provider unmount to avoid a leak). */
  dispose(): void {
    this.cancelErrorClear();
  }

  private scheduleErrorClear(): void {
    this.cancelErrorClear();
    const timer = setTimeout(() => this.clearError(), ERROR_AUTO_HIDE_MS);
    // Don't keep a test/Node process alive solely for the auto-hide timer (no-op in the browser).
    (timer as { unref?: () => void }).unref?.();
    this.errorTimer = timer;
  }

  private cancelErrorClear(): void {
    if (this.errorTimer !== null) {
      clearTimeout(this.errorTimer);
      this.errorTimer = null;
    }
  }

  private computePercent(): number {
    if (this.totalBytes <= 0) return 0;
    return Math.min(100, Math.floor((this.uploadedBytes / this.totalBytes) * 100));
  }

  private emit(): void {
    this.snap = {
      active: this.active,
      fileCount: this.fileCount,
      percent: this.computePercent(),
      error: this.error,
    };
    for (const listener of this.listeners) {
      listener();
    }
  }
}

/** The controller surface exposed to the drop handler. */
export interface UploadProgressController {
  startDrop(fileCount: number, totalBytes: number): void;
  advance(bytes: number): void;
  failFile(fileName: string): void;
  finishDrop(): void;
}

const StoreContext = createContext<UploadProgressStore | null>(null);

/** Provides one shared `UploadProgressStore` to the terminal and the footer. */
export function UploadProgressProvider({ children }: { children: React.ReactNode }) {
  const storeRef = useRef<UploadProgressStore | null>(null);
  if (storeRef.current === null) {
    storeRef.current = new UploadProgressStore();
  }
  const store = storeRef.current;
  useEffect(() => () => store.dispose(), [store]);
  return <StoreContext.Provider value={store}>{children}</StoreContext.Provider>;
}

function useStore(): UploadProgressStore {
  const store = useContext(StoreContext);
  if (store === null) {
    throw new Error("upload-progress hooks must be used within an UploadProgressProvider");
  }
  return store;
}

/** The write side, for the terminal's upload orchestration. */
export function useUploadProgressController(): UploadProgressController {
  const store = useStore();
  return useMemo<UploadProgressController>(
    () => ({
      startDrop: (fileCount, totalBytes) => store.startDrop(fileCount, totalBytes),
      advance: (bytes) => store.advance(bytes),
      failFile: (fileName) => store.failFile(fileName),
      finishDrop: () => store.finishDrop(),
    }),
    [store],
  );
}

/** The read side, for the footer's `UploadProgressIndicator`. */
export function useUploadProgressSnapshot(): UploadProgressSnapshot {
  const store = useStore();
  const subscribe = useCallback((cb: () => void) => store.subscribe(cb), [store]);
  const getSnapshot = useCallback(() => store.snapshot(), [store]);
  return useSyncExternalStore(subscribe, getSnapshot);
}
