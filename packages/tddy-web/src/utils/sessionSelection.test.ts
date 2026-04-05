import { describe, expect, test } from "bun:test";
import {
  computeHeaderCheckboxState,
  toggleRowInTableSelection,
  toggleSelectAllForTable,
} from "./sessionSelection";

describe("sessionSelection helpers (granular)", () => {
  test("computeHeaderCheckboxState: partial selection → indeterminate, not checked", () => {
    expect(computeHeaderCheckboxState(1, 3)).toEqual({ checked: false, indeterminate: true });
  });

  test("computeHeaderCheckboxState: all rows selected → checked, not indeterminate", () => {
    expect(computeHeaderCheckboxState(3, 3)).toEqual({ checked: true, indeterminate: false });
  });

  test("toggleSelectAllForTable: when empty, selects all ids", () => {
    const all = ["a", "b"];
    expect(toggleSelectAllForTable(all, new Set())).toEqual(new Set(all));
  });

  test("toggleSelectAllForTable: when all selected, clears", () => {
    const all = ["a", "b"];
    expect(toggleSelectAllForTable(all, new Set(all))).toEqual(new Set());
  });

  test("computeHeaderCheckboxState: zero rows → unchecked, not indeterminate", () => {
    expect(computeHeaderCheckboxState(0, 0)).toEqual({ checked: false, indeterminate: false });
  });

  test("toggleSelectAllForTable: stale id in selection is replaced by current table ids", () => {
    const all = ["a", "b"];
    const stale = new Set(["a", "b", "removed-from-server"]);
    expect(toggleSelectAllForTable(all, stale)).toEqual(new Set(all));
  });

  test("toggleRowInTableSelection: adds id when absent", () => {
    expect(toggleRowInTableSelection(new Set(), "x")).toEqual(new Set(["x"]));
  });

  test("toggleRowInTableSelection: removes id when present", () => {
    expect(toggleRowInTableSelection(new Set(["x", "y"]), "x")).toEqual(new Set(["y"]));
  });
});
