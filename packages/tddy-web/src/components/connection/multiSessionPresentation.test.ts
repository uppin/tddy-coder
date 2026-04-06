import { describe, expect, it } from "bun:test";
import { detachOthersWhenAddingSecondSession } from "./multiSessionPresentation";

describe("multiSessionPresentation — concurrent attach policy (PRD)", () => {
  it("does not detach existing sessions when adding another (N≥1 unbounded attachments)", () => {
    expect(detachOthersWhenAddingSecondSession()).toBe(false);
  });
});
