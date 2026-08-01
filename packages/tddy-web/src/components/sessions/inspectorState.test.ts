import { describe, it, expect } from "bun:test";
import { defaultInspectorOpen, nextInspectorState } from "./inspectorState";

describe("defaultInspectorOpen", () => {
  // Selecting a session never opens the inspector for the operator: an active session shows its
  // terminal, an inactive one shows its recorded activities, and either way the drawer waits to be
  // asked for. See docs/ft/web/inactive-session-activities.md.
  it("keeps the inspector closed when a session is selected", () => {
    expect(defaultInspectorOpen()).toBe(false);
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

  // Selecting a session no longer depends on its liveness: an active session shows its terminal and
  // an inactive one shows its recorded activities, so the drawer closes either way.
  it("select leaves a closed drawer closed", () => {
    expect(nextInspectorState(closed, { type: "select" })).toEqual({
      open: false,
      expanded: false,
    });
  });

  it("select closes an open drawer", () => {
    expect(nextInspectorState(open, { type: "select" })).toEqual({
      open: false,
      expanded: false,
    });
  });

  it("select collapses an expanded drawer", () => {
    expect(nextInspectorState(expanded, { type: "select" })).toEqual({
      open: false,
      expanded: false,
    });
  });
});
