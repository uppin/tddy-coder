import { describe, expect, it } from "bun:test";
import {
  computeHeaderCheckboxState,
  toggleRowInTableSelection,
  toggleSelectAllForTable,
} from "./sessionSelection";

describe("sessionSelection helpers", () => {
  it("reports indeterminate state when only some rows are selected", () => {
    // When
    const result = computeHeaderCheckboxState(1, 3);
    // Then
    expect(result).toEqual({ checked: false, indeterminate: true });
  });

  it("reports checked state when all rows are selected", () => {
    // When
    const result = computeHeaderCheckboxState(3, 3);
    // Then
    expect(result).toEqual({ checked: true, indeterminate: false });
  });

  it("reports unchecked non-indeterminate state when the table is empty", () => {
    // When
    const result = computeHeaderCheckboxState(0, 0);
    // Then
    expect(result).toEqual({ checked: false, indeterminate: false });
  });

  it("selects all ids when the current selection is empty", () => {
    // Given
    const all = ["a", "b"];

    // When
    const result = toggleSelectAllForTable(all, new Set());

    // Then
    expect(result).toEqual(new Set(all));
  });

  it("clears the selection when all rows are already selected", () => {
    // Given
    const all = ["a", "b"];

    // When
    const result = toggleSelectAllForTable(all, new Set(all));

    // Then
    expect(result).toEqual(new Set());
  });

  it("replaces a stale selection with the current table ids when toggling select-all", () => {
    // Given
    const all = ["a", "b"];
    const stale = new Set(["a", "b", "removed-from-server"]);

    // When
    const result = toggleSelectAllForTable(all, stale);

    // Then
    expect(result).toEqual(new Set(all));
  });

  it("adds a row id to the selection when it is not present", () => {
    // When
    const result = toggleRowInTableSelection(new Set(), "x");
    // Then
    expect(result).toEqual(new Set(["x"]));
  });

  it("removes a row id from the selection when it is already present", () => {
    // Given
    const selection = new Set(["x", "y"]);

    // When
    const result = toggleRowInTableSelection(selection, "x");

    // Then
    expect(result).toEqual(new Set(["y"]));
  });
});
