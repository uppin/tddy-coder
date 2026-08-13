import { describe, expect, it } from "bun:test";
import { projectSelectOptions, type ProjectRow } from "./projectSelectOptions";

/**
 * Aggregated `ListProjects` returns one row per (project_id, hosting daemon) — a deliberate contract
 * (see `packages/tddy-daemon/tests/list_projects_multi_daemon_aggregation.rs`), because the caller
 * needs to know which hosts carry a project. A *project picker* wants the logical project instead, so
 * `projectSelectOptions` reduces those rows to one option per project id.
 *
 * The reduction loses the host, which is chosen by its own "Host" selector, so two rows of the same
 * project must never read as two projects. Two genuinely different projects that happen to share a
 * name are the opposite case: they stay two options, and each is qualified with its id so the
 * operator can tell them apart.
 */
describe("projectSelectOptions", () => {
  it("collapses the rows of one project registered on two hosts into a single option", () => {
    // Given — the same logical project, carried by two daemons
    const rows = [
      aProjectRow({ projectId: "proj-tddy-coder", name: "tddy-coder" }),
      aProjectRow({ projectId: "proj-tddy-coder", name: "tddy-coder" }),
    ];

    // When
    const options = projectSelectOptions(rows);

    // Then
    expect(options).toEqual([{ projectId: "proj-tddy-coder", label: "tddy-coder" }]);
  });

  it("keeps the first row's name when the same project is registered under two different names", () => {
    // Given — one project id, named differently in each host's registry (the `dup-*` collision
    // fixture in cypress/support/rpc/connectionServiceBackend.ts)
    const rows = [
      aProjectRow({ projectId: "proj-dup", name: "dup-workstation" }),
      aProjectRow({ projectId: "proj-dup", name: "dup-server" }),
    ];

    // When
    const options = projectSelectOptions(rows);

    // Then — still one project, and the differing names are not read as a name collision
    expect(options).toEqual([{ projectId: "proj-dup", label: "dup-workstation" }]);
  });

  it("qualifies both labels with the project id when two projects share a name", () => {
    // Given — two unrelated checkouts of a same-named repo
    const rows = [
      aProjectRow({ projectId: "proj-tddy-coder-oss", name: "tddy-coder" }),
      aProjectRow({ projectId: "proj-tddy-coder-fork", name: "tddy-coder" }),
    ];

    // When
    const options = projectSelectOptions(rows);

    // Then
    expect(options).toEqual([
      { projectId: "proj-tddy-coder-oss", label: "tddy-coder (proj-tddy-coder-oss)" },
      { projectId: "proj-tddy-coder-fork", label: "tddy-coder (proj-tddy-coder-fork)" },
    ]);
  });

  it("qualifies only the labels of the names that actually collide", () => {
    // Given — two projects share "tddy-coder"; "tddy-web" is unambiguous on its own
    const rows = [
      aProjectRow({ projectId: "proj-tddy-coder-oss", name: "tddy-coder" }),
      aProjectRow({ projectId: "proj-tddy-coder-fork", name: "tddy-coder" }),
      aProjectRow({ projectId: "proj-tddy-web", name: "tddy-web" }),
    ];

    // When
    const options = projectSelectOptions(rows);

    // Then
    expect(options.map((o) => o.label)).toEqual([
      "tddy-coder (proj-tddy-coder-oss)",
      "tddy-coder (proj-tddy-coder-fork)",
      "tddy-web",
    ]);
  });

  it("falls back to the project id as the label when a project has no name", () => {
    // Given — a registry row whose `name` was never set
    const rows = [aProjectRow({ projectId: "proj-nameless", name: "" })];

    // When
    const options = projectSelectOptions(rows);

    // Then
    expect(options).toEqual([{ projectId: "proj-nameless", label: "proj-nameless" }]);
  });

  it("orders the options by the first appearance of each project id", () => {
    // Given — the local daemon's rows come first, and one project repeats from a peer
    const rows = [
      aProjectRow({ projectId: "proj-tddy-web", name: "tddy-web" }),
      aProjectRow({ projectId: "proj-tddy-coder", name: "tddy-coder" }),
      aProjectRow({ projectId: "proj-tddy-web", name: "tddy-web" }),
    ];

    // When
    const options = projectSelectOptions(rows);

    // Then
    expect(options.map((o) => o.projectId)).toEqual(["proj-tddy-web", "proj-tddy-coder"]);
  });
});

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/** One aggregated `ListProjects` row, reduced to the fields a project picker reads. */
function aProjectRow(overrides: Partial<ProjectRow> = {}): ProjectRow {
  return { projectId: "proj-tddy-coder", name: "tddy-coder", ...overrides };
}
