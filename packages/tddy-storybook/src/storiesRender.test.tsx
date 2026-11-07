import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { composeStories } from "@storybook/react";
import { Glob } from "bun";
import { join } from "node:path";

afterEach(cleanup);

const SRC = import.meta.dir;
const storyFiles = [...new Glob("**/*.stories.tsx").scanSync(SRC)];

describe("stories", () => {
  test("the Storybook has stories", () => {
    // Then
    expect(storyFiles.length).toBeGreaterThan(0);
  });

  for (const file of storyFiles) {
    test(`every story in ${file} renders without an error`, async () => {
      // Given
      const stories = composeStories(await import(join(SRC, file)));

      // When / Then
      for (const [name, Story] of Object.entries(stories)) {
        expect(() => render(<Story />), name).not.toThrow();
        cleanup();
      }
    });
  }
});
