/**
 * A drag over the host application's window, as the page is told about it.
 *
 * Tauri reports positions in physical pixels; the page hit-tests in CSS pixels. On a Retina display
 * those differ by a factor of two, and a drop that is not converted lands on whatever sits at twice
 * the pointer's coordinates — usually not the terminal it was dropped on.
 */

import { describe, expect, it } from "bun:test";
import { nativeDragEventOf } from "./nativeFileDrop.js";

const RETINA = 2;

describe("a native drag step, as a page event", () => {
  it("carries an entering drag's paths and converts its position to CSS pixels", () => {
    // Given
    const payload = {
      type: "enter" as const,
      paths: ["/Users/me/Screen Shot.png"],
      position: { x: 400, y: 300 },
    };

    // When
    const event = nativeDragEventOf(payload, RETINA);

    // Then
    expect(event).toEqual({
      type: "enter",
      paths: ["/Users/me/Screen Shot.png"],
      point: { x: 200, y: 150 },
    });
  });

  it("converts a hovering drag's position", () => {
    // When
    const event = nativeDragEventOf({ type: "over", position: { x: 101, y: 51 } }, RETINA);

    // Then
    expect(event).toEqual({ type: "over", point: { x: 50.5, y: 25.5 } });
  });

  it("carries a drop's paths in the order they were dropped", () => {
    // When
    const event = nativeDragEventOf(
      { type: "drop", paths: ["/b.txt", "/a.txt"], position: { x: 10, y: 20 } },
      1,
    );

    // Then
    expect(event).toEqual({ type: "drop", paths: ["/b.txt", "/a.txt"], point: { x: 10, y: 20 } });
  });

  it("reports a drag leaving the window", () => {
    // When / Then
    expect(nativeDragEventOf({ type: "leave" }, RETINA)).toEqual({ type: "leave" });
  });
});
