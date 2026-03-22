import { describe, expect, test } from "bun:test";
import { bytesForArrowDirection } from "./ghosttyCliArrowCsi";

describe("bytesForArrowDirection (CSI)", () => {
  test("up emits ESC [ A", () => {
    expect(Array.from(bytesForArrowDirection("up"))).toEqual([
      0x1b, 0x5b, 0x41,
    ]);
  });

  test("down emits ESC [ B", () => {
    expect(Array.from(bytesForArrowDirection("down"))).toEqual([
      0x1b, 0x5b, 0x42,
    ]);
  });

  test("right emits ESC [ C", () => {
    expect(Array.from(bytesForArrowDirection("right"))).toEqual([
      0x1b, 0x5b, 0x43,
    ]);
  });

  test("left emits ESC [ D", () => {
    expect(Array.from(bytesForArrowDirection("left"))).toEqual([
      0x1b, 0x5b, 0x44,
    ]);
  });
});
