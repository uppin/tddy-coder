import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

/**
 * Web CI is one workflow (`web.yml`: build, lint, three test shards) plus `storybook.yml`. Packages
 * are discovered from the root `workspaces` by scripts/web-ci.ts, so no per-package workflow exists.
 */

const REPO_ROOT = join(import.meta.dir, "..");
const WORKFLOWS = join(REPO_ROOT, ".github", "workflows");
const read = (file: string) => readFileSync(join(WORKFLOWS, file), "utf8");

describe("web workflows", () => {
  test("no per-package web workflow and no shared package workflow is left", () => {
    // When
    const leftovers = readdirSync(WORKFLOWS).filter((file) => /^web-.+\.yml$/.test(file));

    // Then
    expect(leftovers).toEqual([]);
  });

  test("web.yml has the build, lint and three test-shard checks", () => {
    // Given
    const web = read("web.yml");

    // Then
    expect(web).toMatch(/^\s+name: Web build$/m);
    expect(web).toMatch(/^\s+name: Web lint$/m);
    expect(web).toMatch(/^\s+name: Web tests shard \$\{\{ matrix\.shard \}\}\/3$/m);
    expect(web).toMatch(/^\s+shard: \[1, 2, 3\]$/m);
    expect(web).toContain("bun scripts/web-ci.ts plan --shard");
    expect(web).toContain("--of 3");
  });

  test("a shard's test report is not named like its job", () => {
    // Given — a report check named like the job would be indistinguishable in the rollup.
    const web = read("web.yml");

    // When
    const reportName = /check_name: (.+)$/m.exec(web)?.[1] ?? "";

    // Then
    expect(reportName).not.toMatch(/^Web tests/);
    expect(reportName).toContain("report");
  });

  test("storybook.yml builds the Storybook as its own check", () => {
    // Given
    const storybook = read("storybook.yml");

    // Then
    expect(storybook).toMatch(/^\s+name: Storybook build$/m);
    expect(storybook).toContain("build-storybook");
  });

  test("both web workflows run on PRs, master and dispatch, behind the build gate", () => {
    for (const file of ["web.yml", "storybook.yml"]) {
      // Given
      const body = read(file);

      // Then
      expect(body).toMatch(/types: \[opened, synchronize, reopened, ready_for_review\]/);
      expect(body).toMatch(/branches: \[master\]/);
      expect(body).toMatch(/^\s+workflow_dispatch:/m);
      expect(body).toContain("uses: ./.github/workflows/build-gate.yml");
      expect(body).toMatch(/^concurrency:\s*$/m);
      expect(body).toMatch(/^\s+pull-requests: read\b/m);
    }
  });

  test("ci.yml does not run the web suites", () => {
    expect(read("ci.yml")).not.toMatch(/^\s{2}web:\s*$/m);
  });
});
