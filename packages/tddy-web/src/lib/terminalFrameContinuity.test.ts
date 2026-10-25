/**
 * Unit tests for the offset-continuity guard a snapshot-opened terminal stream runs every frame
 * through. Feature: docs/ft/daemon/terminal-sessions.md (Resync at any point)
 */

import { describe, expect, it } from "bun:test";
import { TerminalFrameContinuity, type OffsetStampedFrame } from "./terminalFrameContinuity";

const aSnapshotAt = (endOffset: bigint): OffsetStampedFrame => ({ snapshot: true, startOffset: 0n, endOffset });
const aDelta = (startOffset: bigint, length: number): OffsetStampedFrame => ({
  snapshot: false,
  startOffset,
  endOffset: startOffset + BigInt(length),
});

function verdictsFor(frames: OffsetStampedFrame[]): string[] {
  const guard = new TerminalFrameContinuity();
  return frames.map((frame) => guard.accept(frame));
}

function actionsFor(frames: OffsetStampedFrame[]): string[] {
  const guard = new TerminalFrameContinuity();
  return frames.map((frame) => guard.route(frame));
}

describe("terminal frame continuity", () => {
  it("paints a snapshot and the deltas that follow it without a hole", () => {
    expect(verdictsFor([aSnapshotAt(100n), aDelta(100n, 4), aDelta(104n, 10)])).toEqual([
      "paint",
      "paint",
      "paint",
    ]);
  });

  it("refuses a delta that starts past where the last frame ended", () => {
    expect(verdictsFor([aSnapshotAt(100n), aDelta(100n, 4), aDelta(110n, 3)])).toEqual([
      "paint",
      "paint",
      "gap",
    ]);
  });

  it("refuses a delta that repeats bytes the screen already has", () => {
    expect(verdictsFor([aSnapshotAt(100n), aDelta(100n, 4), aDelta(102n, 6)])).toEqual([
      "paint",
      "paint",
      "gap",
    ]);
  });

  it("refuses a delta before any snapshot arrived", () => {
    expect(verdictsFor([aDelta(0n, 4)])).toEqual(["gap"]);
  });

  it("refuses every delta after a gap until a snapshot, then paints again from it", () => {
    expect(
      verdictsFor([aSnapshotAt(100n), aDelta(120n, 4), aDelta(124n, 4), aSnapshotAt(300n), aDelta(300n, 2)]),
    ).toEqual(["paint", "gap", "gap", "paint", "paint"]);
  });

  it("refuses a delta that carries no start offset", () => {
    expect(verdictsFor([aSnapshotAt(100n), { endOffset: 0n }])).toEqual(["paint", "gap"]);
  });
});

describe("what a feed does with each frame", () => {
  it("asks for one snapshot per run of gapped frames", () => {
    expect(actionsFor([aSnapshotAt(100n), aDelta(120n, 4), aDelta(124n, 4), aDelta(128n, 4)])).toEqual([
      "paint",
      "drop-and-ask",
      "drop",
      "drop",
    ]);
  });

  it("asks again on a gap after the snapshot that answered the last ask", () => {
    expect(
      actionsFor([aSnapshotAt(100n), aDelta(120n, 4), aSnapshotAt(300n), aDelta(300n, 2), aDelta(400n, 1)]),
    ).toEqual(["paint", "drop-and-ask", "paint", "paint", "drop-and-ask"]);
  });
});
