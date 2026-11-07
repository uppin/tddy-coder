import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { composeStories, setProjectAnnotations } from "@storybook/react";
import * as previewAnnotations from "../.storybook/preview";
import { Glob } from "bun";
import { join } from "node:path";

// The default world every story renders in: the port doubles `.storybook/preview.tsx` mounts.
setProjectAnnotations(previewAnnotations);

afterEach(cleanup);

const SRC = import.meta.dir;
/**
 * Stories that need a real browser: ghostty-web instantiates a WebAssembly module over `fetch` and
 * draws to a canvas, neither of which happy-dom provides. They render in the static Storybook build,
 * which the tddy-web e2e specs (`ghostty-terminal-stories.cy.ts`, ...) drive.
 */
const NEEDS_A_BROWSER = ["tddy-terminal-web/GhosttyTerminal.stories.tsx"];

const storyFiles = [...new Glob("**/*.stories.tsx").scanSync(SRC)].filter((f) => !NEEDS_A_BROWSER.includes(f));

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
