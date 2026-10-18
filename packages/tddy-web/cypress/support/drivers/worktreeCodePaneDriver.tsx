/**
 * Fluent driver for the session Code pane (`WorktreeCodePane`) mounted on its own, over a
 * `CodeIndexHost` — a worktree's files plus the `CodeSession` the pane holds open with its index.
 *
 * Usage:
 *
 *   const host = aCodeIndexHost().withFile("src/main.rs", MAIN_RS).startingAt(anIndexReady());
 *   const pane = aCodePaneOn(host).mount().previewing("src/main.rs");
 *   pane.dwellOn({ line: 4, column: 26 });
 *   ...
 *   pane.close();
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { CodeNavigationService } from "../../../src/gen/code_navigation_pb";
import { WorktreeService } from "../../../src/gen/worktree_pb";
import { WorktreeCodePane } from "../../../src/components/session/WorktreeCodePane";
import { mountWithRpc } from "../rpc/inMemory";
import type { CodeIndexHost, OpenedWorktree } from "../rpc/codeSessionBackend";
import { worktreeCodePanePage } from "../pages/worktreeCodePanePage";

/** The worktree every pane this driver mounts is opened on. */
export const THE_WORKTREE: OpenedWorktree = {
  sessionToken: "fake-token",
  projectId: "proj-code-session",
  worktreePath: "/home/dev/repo/.worktrees/code-session",
};

/** The pane's outer size, in CSS pixels. */
export interface PaneSize {
  width: number;
  height: number;
}

const DEFAULT_SIZE: PaneSize = { width: 900, height: 600 };

/** A one-based line and one-based byte column in the previewed file. */
export interface PreviewPoint {
  line: number;
  column: number;
}

/**
 * What a user does in the Code pane's preview, wherever the pane is mounted — on its own (the
 * driver below) or inside the sessions drawer.
 */
export const codePreview = {
  /** Rest the pointer on the identifier at `at` (the pane asks about it after its dwell). */
  dwellOn(at: PreviewPoint) {
    worktreeCodePanePage.identifier(at.line, at.column).trigger("mouseover");
  },

  /** Ctrl-click the identifier at `at`: go to its definition. */
  goToDefinitionOf(at: PreviewPoint) {
    worktreeCodePanePage.identifier(at.line, at.column).click({ ctrlKey: true });
  },

  /** Click the identifier at `at` with no modifier key. */
  plainClick(at: PreviewPoint) {
    worktreeCodePanePage.identifier(at.line, at.column).click();
  },

  /** List the references of the hovered identifier from its card. */
  listReferences() {
    worktreeCodePanePage.referencesAction().click();
  },

  /** Close the hover card. */
  closeHoverCard() {
    worktreeCodePanePage.hoverClose().click();
  },
};

export function aCodePaneOn(host: CodeIndexHost) {
  const driver = {
    /** Mount the pane, `size` large (default 900×600). */
    mount(size: PaneSize = DEFAULT_SIZE) {
      const backend = host.backend();
      mountWithRpc(
        <div style={{ height: size.height, width: size.width, display: "flex" }}>
          <WorktreeCodePane
            client={createClient(WorktreeService, backend.transport())}
            sessionToken={THE_WORKTREE.sessionToken}
            projectId={THE_WORKTREE.projectId}
            worktreePath={THE_WORKTREE.worktreePath}
            navigationClient={createClient(CodeNavigationService, backend.transport())}
          />
        </div>,
        backend,
      );
      return driver;
    },

    /** Open `relPath` in the preview by walking the tree to it. */
    previewing(relPath: string) {
      const segments = relPath.split("/");
      segments.forEach((_, i) => {
        worktreeCodePanePage.node(segments.slice(0, i + 1).join("/")).click();
      });
      worktreeCodePanePage.highlight().should("be.visible");
      return driver;
    },

    /** Open `relPath`, whose directory the tree already shows, in the preview. */
    opening(relPath: string) {
      worktreeCodePanePage.node(relPath).click();
      worktreeCodePanePage.highlight().should("be.visible");
      return driver;
    },

    /** Rest the pointer on the identifier at `at` (the pane asks about it after its dwell). */
    dwellOn(at: PreviewPoint) {
      codePreview.dwellOn(at);
      return driver;
    },

    /** Ctrl-click the identifier at `at`: go to its definition. */
    goToDefinitionOf(at: PreviewPoint) {
      codePreview.goToDefinitionOf(at);
      return driver;
    },

    /** Click the identifier at `at` with no modifier key. */
    plainClick(at: PreviewPoint) {
      codePreview.plainClick(at);
      return driver;
    },

    /** Close the hover card. */
    closeHoverCard() {
      codePreview.closeHoverCard();
      return driver;
    },

    /** Unmount the pane, as closing the Code toggle does. */
    close() {
      cy.mount(<div />);
      return driver;
    },
  };
  return driver;
}
