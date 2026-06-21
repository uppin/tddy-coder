import { describe, it, expect } from "bun:test";
import { defaultInspectorOpen, nextInspectorState } from "./inspectorState";

describe("defaultInspectorOpen", () => {
  it("returns false for an active session (connected — inspector is hidden by default)", () => {
    expect(defaultInspectorOpen(true)).toBe(false);
  });

  it("returns true for an inactive session (disconnected — inspector is open by default)", () => {
    expect(defaultInspectorOpen(false)).toBe(true);
  });
});

describe("nextInspectorState reducer", () => {
  const closed = { open: false, expanded: false };
  const open = { open: true, expanded: false };
  const expanded = { open: true, expanded: true };

  it("open action opens a closed drawer", () => {
    expect(nextInspectorState(closed, { type: "open" })).toEqual({
      open: true,
      expanded: false,
    });
  });

  it("close action closes an open drawer", () => {
    expect(nextInspectorState(open, { type: "close" })).toEqual({
      open: false,
      expanded: false,
    });
  });

  it("close action closes an expanded drawer", () => {
    expect(nextInspectorState(expanded, { type: "close" })).toEqual({
      open: false,
      expanded: false,
    });
  });

  it("toggle opens a closed drawer", () => {
    expect(nextInspectorState(closed, { type: "toggle" })).toEqual({
      open: true,
      expanded: false,
    });
  });

  it("toggle closes an open drawer", () => {
    expect(nextInspectorState(open, { type: "toggle" })).toEqual({
      open: false,
      expanded: false,
    });
  });

  it("expand action expands an open drawer", () => {
    expect(nextInspectorState(open, { type: "expand" })).toEqual({
      open: true,
      expanded: true,
    });
  });

  it("restore action returns an expanded drawer to open", () => {
    expect(nextInspectorState(expanded, { type: "restore" })).toEqual({
      open: true,
      expanded: false,
    });
  });

  it("select with isActive=false opens the drawer (disconnected session default)", () => {
    expect(nextInspectorState(closed, { type: "select", isActive: false })).toEqual({
      open: true,
      expanded: false,
    });
  });

  it("select with isActive=true closes the drawer (connected session default)", () => {
    expect(nextInspectorState(open, { type: "select", isActive: true })).toEqual({
      open: false,
      expanded: false,
    });
  });

  it("select always resets expanded to false", () => {
    expect(nextInspectorState(expanded, { type: "select", isActive: false })).toEqual({
      open: true,
      expanded: false,
    });
  });
});
