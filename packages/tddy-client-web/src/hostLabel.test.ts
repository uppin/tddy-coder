/**
 * Unit tests for the one rule a host is named by in text — an error, a badge, a gate title.
 */

import { describe, expect, it } from "bun:test";
import { hostLabelOf, type HostLabelSource } from "./hostLabel";

function aDirectory(overrides: Partial<HostLabelSource> = {}): HostLabelSource {
  return {
    daemons: [
      { instanceId: "workstation-1", label: "workstation-1 (this daemon)" },
      { instanceId: "server-2", label: "build box" },
    ],
    selectedInstanceId: "workstation-1",
    selectedLabel: "workstation-1 (this daemon)",
    ...overrides,
  };
}

describe("hostLabelOf", () => {
  it("names a listed host by its label, without the self-advertised ' (this daemon)' suffix", () => {
    // Given every daemon advertises itself from its own perspective
    const directory = aDirectory();

    // When
    const label = hostLabelOf(directory, "workstation-1");

    // Then
    expect(label).toBe("workstation-1");
  });

  it("names a listed host whose label carries no suffix by that label", () => {
    expect(hostLabelOf(aDirectory(), "server-2")).toBe("build box");
  });

  it("names the selected host by the label it was last seen under while it is not listed", () => {
    // Given the selected host dropped out of the directory
    const directory = aDirectory({ daemons: [], selectedLabel: "workstation-1 (this daemon)" });

    // When
    const label = hostLabelOf(directory, "workstation-1");

    // Then
    expect(label).toBe("workstation-1");
  });

  it("names a host nobody has listed by its instance id", () => {
    expect(hostLabelOf(aDirectory(), "never-seen")).toBe("never-seen");
  });

  it("names no host at all as the empty string", () => {
    expect(hostLabelOf(aDirectory(), "")).toBe("");
  });
});
