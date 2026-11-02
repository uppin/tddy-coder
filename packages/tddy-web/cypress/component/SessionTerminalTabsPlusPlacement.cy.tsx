/**
 * Acceptance: the tab strip's "+" sits right after the last tab, and stays reachable when the tabs
 * overflow — sticky at the scroller's right edge — with its menu never clipped by the scroller.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-02-tab-plus-follows-last-tab.md (`#session-tabs 5/7`)
 *
 * The strip is mounted on its own, without RPC: placement is a rendering question. The app
 * stylesheet is imported here, in this spec only, because the overflow and sticky cases need real
 * Tailwind layout (the shared component harness loads no CSS — see
 * docs/dev/todo/2026-08-01-tddy-web-inactive-session-activities-follow-ups.md).
 */

import "../../src/index.css";
import React from "react";
import { SessionTerminalTabs } from "../../src/components/sessions/SessionTerminalTabs";
import type { AgentConversation } from "../../src/components/sessions/agentConversationTabs";
import {
  AGENT_TERMINAL_ID,
  type SessionTerminal,
} from "../../src/components/sessions/useSessionTerminals";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

function bashTerminals(count: number): SessionTerminal[] {
  return Array.from({ length: count }, (_, i) => ({ terminalId: `bash-${i + 1}`, kind: "bash" }));
}

function aConversation(conversationId: string): AgentConversation {
  return { agentId: "explorer@local", conversationId, label: "", daemonInstanceId: "local" };
}

/** Mount the strip `width` pixels wide with these tabs. */
function mountTheStrip(
  {
    terminals = [],
    conversations = [],
  }: { terminals?: SessionTerminal[]; conversations?: AgentConversation[] },
  width = 1200,
) {
  cy.mount(
    <div style={{ width }}>
      <SessionTerminalTabs
        terminals={terminals}
        activeTerminalId={AGENT_TERMINAL_ID}
        onSelect={cy.stub()}
        onOpen={cy.stub().as("onOpen")}
        onClose={cy.stub()}
        agentConversations={conversations}
        activeAgentConversationId={null}
        onSelectAgentConversation={cy.stub()}
        onCloseAgentConversation={cy.stub()}
      />
    </div>,
  );
}

/**
 * Assert "+" is inside the scrolling tab list, after `lastTab` in document order, with no tab of any
 * kind after it.
 */
function expectPlusRightAfter(lastTab: Cypress.Chainable<JQuery<HTMLElement>>) {
  lastTab.then(($last) => {
    const scroller = $last[0].closest(".overflow-x-auto");
    expect(scroller, "the tab scroller").to.not.equal(null);
    tabs.newTab().then(($plus) => {
      const plus = $plus[0];
      expect(scroller!.contains(plus), '"+" is inside the tab scroller').to.equal(true);
      expect(
        $last[0].compareDocumentPosition(plus) & Node.DOCUMENT_POSITION_FOLLOWING,
        '"+" follows the last tab',
      ).to.not.equal(0);
      const tabsAfterPlus = Array.from(scroller!.querySelectorAll("[role='tab']")).filter(
        (tab) => plus.compareDocumentPosition(tab) & Node.DOCUMENT_POSITION_FOLLOWING,
      );
      expect(tabsAfterPlus, 'tabs after "+"').to.have.length(0);
    });
  });
}

// ---------------------------------------------------------------------------

describe('The tab strip\'s "+" follows the last tab', () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it('"+" follows the Agent tab when it is the only tab', () => {
    mountTheStrip({});

    expectPlusRightAfter(tabs.agentTab());
  });

  it('"+" follows the last terminal tab', () => {
    mountTheStrip({ terminals: bashTerminals(2) });

    expectPlusRightAfter(tabs.tab("bash-2"));
  });

  it('"+" follows the last agent-conversation tab', () => {
    mountTheStrip({ terminals: bashTerminals(1), conversations: [aConversation("conv-1")] });

    expectPlusRightAfter(tabs.agentConversationTab("conv-1"));
  });

  it('"+" stays visible at the scroller\'s edge when the tabs overflow', () => {
    // Given far more tabs than fit in 500px, scrolled to the start
    mountTheStrip({ terminals: bashTerminals(30) }, 500);
    tabs.agentTab().then(($agent) => {
      $agent[0].closest(".overflow-x-auto")!.scrollLeft = 0;
    });

    // Then "+" is still on screen, inside the strip's width
    tabs.newTab().should("be.visible");
    tabs.tabs().then(($strip) => {
      const strip = $strip[0].getBoundingClientRect();
      tabs.newTab().then(($plus) => {
        const plus = $plus[0].getBoundingClientRect();
        expect(plus.right, '"+" right edge').to.be.at.most(strip.right);
        expect(plus.left, '"+" left edge').to.be.at.least(strip.left);
      });
    });
  });

  it('the "+" menu is fully visible while the tabs overflow', () => {
    // Given an overflowing strip
    mountTheStrip({ terminals: bashTerminals(30) }, 500);

    // When the menu is opened
    tabs.newTab().click();

    // Then every item of it is visible and it is not inside the clipping scroller
    tabs.newTabMenu().should("be.visible");
    tabs.newTabOption("bash").should("be.visible");
    tabs.newTabMenu().then(($menu) => {
      expect($menu[0].closest(".overflow-x-auto"), "the menu's clipping ancestor").to.equal(null);
    });
  });

  it("Escape and an outside click still close the menu", () => {
    mountTheStrip({ terminals: bashTerminals(30) }, 500);

    tabs.newTab().click();
    tabs.newTabMenu().should("be.visible");
    cy.get("body").type("{esc}");
    tabs.newTabMenu().should("not.exist");

    tabs.newTab().click();
    tabs.newTabMenu().should("be.visible");
    tabs.agentTab().click();
    tabs.newTabMenu().should("not.exist");
  });
});
