/**
 * Unit tests for the upload-progress store that feeds the bottom-strip indicator.
 * The store is shared (via a provider) between the terminal's drop handler and
 * the Host Stats Footer, so its accounting is tested here in isolation.
 *
 * Changeset: `terminal-file-drop-upload`
 * PRD: docs/ft/web/host-stats-footer.md § Upload progress
 */

import { describe, it, expect, beforeEach } from "bun:test";
import { UploadProgressStore } from "./uploadProgress";

describe("UploadProgressStore", () => {
  let store: UploadProgressStore;

  beforeEach(() => {
    store = new UploadProgressStore();
  });

  it("starts inactive with no error and zero percent", () => {
    const snap = store.snapshot();
    expect(snap.active).toBe(false);
    expect(snap.error).toBe(null);
    expect(snap.percent).toBe(0);
  });

  it("becomes active for the started drop's file count", () => {
    store.startDrop(3, 900);
    const snap = store.snapshot();
    expect(snap.active).toBe(true);
    expect(snap.fileCount).toBe(3);
    expect(snap.percent).toBe(0);
  });

  it("reports floored percent as bytes are advanced", () => {
    store.startDrop(1, 200);
    store.advance(50);
    expect(store.snapshot().percent).toBe(25);
    store.advance(51);
    expect(store.snapshot().percent).toBe(50); // floor(101/200*100)
  });

  it("caps percent at 100 even if advance overshoots the total", () => {
    store.startDrop(1, 100);
    store.advance(250);
    expect(store.snapshot().percent).toBe(100);
  });

  it("records a failed file by name in the error message", () => {
    store.startDrop(2, 100);
    store.failFile("report.iso");
    expect(store.snapshot().error).toContain("report.iso");
  });

  it("deactivates on finishDrop but preserves a recorded error", () => {
    store.startDrop(2, 100);
    store.failFile("bad.txt");
    store.finishDrop();
    const snap = store.snapshot();
    expect(snap.active).toBe(false);
    expect(snap.error).toContain("bad.txt");
  });

  it("clears a prior error when a new drop starts", () => {
    store.startDrop(1, 10);
    store.failFile("old.txt");
    store.finishDrop();
    store.startDrop(1, 10);
    expect(store.snapshot().error).toBe(null);
  });

  it("notifies subscribers on each transition", () => {
    let notifications = 0;
    const unsubscribe = store.subscribe(() => {
      notifications += 1;
    });
    store.startDrop(1, 10);
    store.advance(5);
    unsubscribe();
    store.advance(5);
    expect(notifications).toBe(2);
  });
});
