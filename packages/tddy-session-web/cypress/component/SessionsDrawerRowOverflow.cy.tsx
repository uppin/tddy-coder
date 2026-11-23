/**
 * Acceptance: whatever sits on the right of a drawer row — the project header's "+" and gear;
 * a session row's host and orchestrator badges; the pinned main entry's model — stays inside the
 * drawer however long the label and however narrow the drawer, and the label truncates instead.
 * The pinned **main** entry's status dot lines up with the session rows' dots.
 *
 * PRD: docs/ft/web/session-drawer.md
 *
 * The app stylesheet is imported here, in this spec only, because these are real-layout
 * measurements (the shared component harness loads no CSS).
 */

import "tddy-ui-web/theme.css"; // measures real layout, so it needs the stylesheet the others run without
import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "tddy-client-web/gen/session_pb";
import { TooltipProvider } from "tddy-ui-web";
import { SessionDrawer } from "../../src/SessionDrawer";
import { ORCHESTRATOR_ROLE } from "../../src/utils/projectHomeGroups";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PROJECT_ID = "proj-long";
const PROJECT_NAME = "tddy-coder-with-a-project-name-far-wider-than-any-drawer";
const HOME_ID = "home-0000-4000-8000-000000000001";
const CROSS_HOST_ID = "xhst-0000-4000-8000-000000000002";

function aSession(sessionId: string, extra: Partial<SessionEntry> = {}): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId,
    createdAt: "2026-11-15T10:00:00Z",
    status: "active",
    repoPath: "/home/dev/project",
    pid: 1,
    isActive: true,
    projectId: PROJECT_ID,
    daemonInstanceId: "local",
    ...extra,
  });
}

const HOME_SESSION = aSession(HOME_ID, {
  repoPath: "/home/dev/main",
  model: "claude-opus-with-an-unreasonably-long-model-identifier",
});

/** A cross-host orchestrator row whose label alone is wider than the drawer. */
const CROSS_HOST_SESSION = aSession(CROSS_HOST_ID, {
  repoPath: "/home/dev/investigate-split-session-udoo-and-then-some-more-words-to-overflow",
  daemonInstanceId: "manta-macbook",
  sessionRole: ORCHESTRATOR_ROLE,
});

// ---------------------------------------------------------------------------
// Mount
// ---------------------------------------------------------------------------

type Layout = { name: string; width: number; isMobile: boolean };

/** The desktop column (its own fixed width) and the mobile overlay in a narrow container. */
const LAYOUTS: Layout[] = [
  { name: "desktop column", width: 600, isMobile: false },
  { name: "240px mobile overlay", width: 240, isMobile: true },
];

function mountTheDrawer({ width, isMobile }: Layout) {
  cy.viewport(1000, 700);
  cy.mount(
    <TooltipProvider delayDuration={0}>
      <div style={{ position: "relative", width, height: 600, display: "flex" }}>
        <SessionDrawer
          sessions={[HOME_SESSION, CROSS_HOST_SESSION]}
          selectedSessionId={null}
          onSelectSession={cy.stub()}
          isOpen
          isMobile={isMobile}
          onClose={cy.stub()}
          onOpen={cy.stub()}
          crossHostSessionsVisible
          selectedInstanceId="local"
          hostLabelForInstance={(id) => `${id}-pro.local`}
          projects={[{ projectId: PROJECT_ID, name: PROJECT_NAME }]}
          projectHomes={[{ projectId: PROJECT_ID, mainBranchRef: "origin/master", homeSessionId: HOME_ID }]}
          onEnsureProjectHome={() => Promise.resolve()}
          onNewOrchestrator={cy.stub()}
          onOpenProjectSettings={cy.stub()}
        />
      </div>
    </TooltipProvider>,
  );
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

/** Every element `$el` matches lies wholly inside the drawer's visible rectangle. */
function expectInsideTheDrawer($el: JQuery<HTMLElement>, what: string, opts: { mayBeCollapsed?: boolean } = {}) {
  const drawer = document.querySelector("[data-testid='sessions-drawer']")!.getBoundingClientRect();
  expect($el.length, what).to.be.greaterThan(0);
  $el.each((_, el) => {
    const r = el.getBoundingClientRect();
    if (!opts.mayBeCollapsed) expect(r.width, `${what} has a width`).to.be.greaterThan(0);
    expect(r.left, `${what} left edge`).to.be.at.least(drawer.left - 0.5);
    expect(r.right, `${what} right edge (drawer right ${drawer.right})`).to.be.at.most(drawer.right + 0.5);
  });
}

function expectTruncated($el: JQuery<HTMLElement>, what: string) {
  const el = $el[0];
  expect(el.scrollWidth, `${what} scrollWidth > clientWidth`).to.be.greaterThan(el.clientWidth);
  expect(getComputedStyle(el).textOverflow, `${what} text-overflow`).to.equal("ellipsis");
}

const header = () => byTestId(`sessions-drawer-project-group-header-${PROJECT_ID}`);
const mainEntry = () => byTestId(`sessions-drawer-project-home-${PROJECT_ID}`);
const crossHostRow = () => byTestId(`sessions-drawer-item-${CROSS_HOST_ID}`);

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

for (const layout of LAYOUTS) {
  describe(`Drawer rows keep their right-hand items in view — ${layout.name}`, () => {
    beforeEach(() => {
      cy.clearLocalStorage();
      mountTheDrawer(layout);
    });

    it("the project header's \"+\" and gear stay inside the drawer, and the name truncates", () => {
      byTestId(`sessions-drawer-project-new-${PROJECT_ID}`).then(($plus) => expectInsideTheDrawer($plus, '"+"'));
      byTestId(`sessions-drawer-project-settings-${PROJECT_ID}`).then(($gear) =>
        // Zero-width until the header row is hovered (CSS :hover cannot be synthesised here).
        expectInsideTheDrawer($gear, "settings gear", { mayBeCollapsed: true }),
      );
      header().find("> span.truncate").then(($name) => expectTruncated($name, "project name"));
    });

    it("a cross-host row's host and orchestrator badges stay inside the drawer, and its label truncates", () => {
      byTestId(`sessions-drawer-item-host-${CROSS_HOST_ID}`).then(($host) => expectInsideTheDrawer($host, "host badge"));
      byTestId(`sessions-drawer-item-orchestrator-${CROSS_HOST_ID}`).then(($orch) =>
        expectInsideTheDrawer($orch, "orchestrator badge"),
      );
      byTestId(`sessions-drawer-item-label-${CROSS_HOST_ID}`).then(($label) => {
        expectTruncated($label, "session label");
        expect($label[0].clientWidth, "session label keeps some width").to.be.greaterThan(0);
      });
    });

    it("the main entry's model stays inside the drawer", () => {
      mainEntry().find("> span:last-child").then(($model) => expectInsideTheDrawer($model, "main's model"));
    });

    it("the main entry's status dot lines up with the session rows' dots", () => {
      mainEntry().find("> :first-child").then(($mainDot) => {
        crossHostRow().find("> :first-child").then(($rowDot) => {
          const main = $mainDot[0].getBoundingClientRect();
          const row = $rowDot[0].getBoundingClientRect();
          expect(main.left, "main dot left vs session row dot left").to.be.closeTo(row.left, 0.5);
        });
      });
    });
  });
}
