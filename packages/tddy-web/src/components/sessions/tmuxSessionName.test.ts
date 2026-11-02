import { describe, expect, it } from "bun:test";
import { nextTmuxSessionName } from "./tmuxSessionName";

describe("nextTmuxSessionName", () => {
  it("offers the base when no tmux session has it", () => {
    expect(nextTmuxSessionName("a1b2c3", ["other"])).toBe("a1b2c3");
  });

  it("offers base-2 when the base is taken", () => {
    expect(nextTmuxSessionName("a1b2c3", ["a1b2c3"])).toBe("a1b2c3-2");
  });

  it("offers the first free suffix", () => {
    expect(nextTmuxSessionName("a1b2c3", ["a1b2c3", "a1b2c3-2", "a1b2c3-4"])).toBe("a1b2c3-3");
  });
});
