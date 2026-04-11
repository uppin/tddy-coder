import { describe, expect, it } from "bun:test";
import {
  HOME_PROJECT_SESSIONS_PREVIEW_LIMIT,
  splitSortedSessionsForHomePreview,
} from "./projectSessionsHomePreview";

describe("splitSortedSessionsForHomePreview (granular — RED)", () => {
  it("granular: when total exceeds limit, visible length is capped and hiddenCount is total - limit", () => {
    const sorted = Array.from({ length: 12 }, (_, i) => ({ n: i }));
    const r = splitSortedSessionsForHomePreview(sorted);
    expect(r.visible.length).toBe(HOME_PROJECT_SESSIONS_PREVIEW_LIMIT);
    expect(r.total).toBe(12);
    expect(r.hiddenCount).toBe(2);
  });

  it("granular: when total is limit + 1, one session is hidden", () => {
    const sorted = Array.from({ length: 11 }, (_, i) => ({ n: i }));
    const r = splitSortedSessionsForHomePreview(sorted);
    expect(r.visible.length).toBe(HOME_PROJECT_SESSIONS_PREVIEW_LIMIT);
    expect(r.hiddenCount).toBe(1);
  });
});
