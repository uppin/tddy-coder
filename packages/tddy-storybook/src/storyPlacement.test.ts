import { describe, expect, test } from "bun:test";
import { execFileSync } from "node:child_process";
import { join } from "node:path";

const REPO_ROOT = join(import.meta.dir, "../../..");

function trackedStories(): string[] {
  try {
    return execFileSync("git", ["ls-files", "packages/*/src/**/*.stories.tsx", "packages/*/src/**/*.stories.ts"], {
      cwd: REPO_ROOT,
      encoding: "utf8",
    })
      .split("\n")
      .filter(Boolean);
  } catch {
    return [];
  }
}

describe("story placement", () => {
  test("every story lives in tddy-storybook", () => {
    // When
    const elsewhere = trackedStories().filter((f) => !f.startsWith("packages/tddy-storybook/"));

    // Then
    expect(elsewhere).toEqual([]);
  });

  test("every component package has at least one story", () => {
    // Given
    const packages = [
      "tddy-ui-web",
      "tddy-terminal-web",
      "tddy-chat-web",
      "tddy-hosts-web",
      "tddy-workspace-web",
      "tddy-prstack-web",
      "tddy-session-web",
      "tddy-session-create-web",
      "tddy-rpc-playground-web",
      "tddy-livekit-web",
    ];

    // When
    const stories = trackedStories().filter((f) => f.startsWith("packages/tddy-storybook/"));
    const withoutStory = packages.filter((p) => !stories.some((s) => s.includes(`/src/${p}/`)));

    // Then
    expect(withoutStory).toEqual([]);
  });
});
