import React, { useCallback, useId, useState } from "react";
import type { ClientMessage } from "../../gen/tddy/v1/remote_pb";
import {
  submitPlanApprove,
  submitPlanRefinement,
  submitPlanReject,
} from "./planReviewActions";
import { logPlanReviewMarker } from "./planReviewMarkers";

export type PlanReviewActivityPaneProps = {
  prdMarkdown: string;
  onRefineSubmit: (text: string) => void;
  onApprove: () => void;
  onReject: () => void;
  onClientMessage?: (message: ClientMessage) => void;
};

/** Shared dark-theme tokens for the plan-review pane (hex literals live here only). */
const planReviewTheme = {
  bgDeep: "#0d0d0d",
  bgTerminal: "#0a0a0a",
  bgPlan: "#121212",
  bgFooter: "#141414",
  borderSubtle: "#2a2a2a",
  textPrimary: "#e8e8e8",
  inputBorder: "#444",
  btnBorder: "#555",
  primaryBg: "#1f5f3a",
  primaryBorder: "#2d8a56",
  dangerBg: "#5c2020",
  dangerBorder: "#8a3a3a",
} as const;

const shell: React.CSSProperties = {
  boxSizing: "border-box",
  display: "flex",
  flexDirection: "row",
  width: "100%",
  maxWidth: "100%",
  /** Keep pane under full-viewport takeover (Cypress compares to config viewport × 0.92). */
  height: "min(420px, 75vh)",
  maxHeight: "min(420px, 75vh)",
  minHeight: 200,
  overflow: "hidden",
  background: planReviewTheme.bgDeep,
  color: planReviewTheme.textPrimary,
};

/**
 * Each column stays under ~92% of Cypress `viewportWidth` (component runner may report a
 * config width smaller than the iframe; narrow caps keep layout tests stable).
 */
const terminalRegion: React.CSSProperties = {
  flex: "1 1 0",
  minWidth: 0,
  maxWidth: "min(42%, 440px)",
  minHeight: 120,
  background: planReviewTheme.bgTerminal,
  borderRight: `1px solid ${planReviewTheme.borderSubtle}`,
};

const planPane: React.CSSProperties = {
  flex: "1 1 0",
  minWidth: 0,
  maxWidth: "min(42%, 440px)",
  display: "flex",
  flexDirection: "column",
  position: "relative",
  minHeight: 0,
  background: planReviewTheme.bgPlan,
};

const markdownScroll: React.CSSProperties = {
  flex: 1,
  minHeight: 0,
  overflow: "auto",
  padding: "12px 16px",
  borderBottom: `1px solid ${planReviewTheme.borderSubtle}`,
};

const markdownPre: React.CSSProperties = {
  margin: 0,
  whiteSpace: "pre-wrap",
  wordBreak: "break-word",
  fontFamily: "ui-sans-serif, system-ui, sans-serif",
  fontSize: 14,
  lineHeight: 1.5,
};

const refineRow: React.CSSProperties = {
  display: "flex",
  flexDirection: "row",
  gap: 8,
  padding: "10px 12px",
  alignItems: "center",
  borderBottom: `1px solid ${planReviewTheme.borderSubtle}`,
  flexShrink: 0,
};

const footer: React.CSSProperties = {
  display: "flex",
  flexDirection: "row",
  gap: 10,
  padding: "12px 16px",
  flexShrink: 0,
  background: planReviewTheme.bgFooter,
  borderTop: `1px solid ${planReviewTheme.borderSubtle}`,
};

const inputStyle: React.CSSProperties = {
  flex: 1,
  minWidth: 0,
  padding: "8px 10px",
  borderRadius: 4,
  border: `1px solid ${planReviewTheme.inputBorder}`,
  background: planReviewTheme.bgDeep,
  color: planReviewTheme.textPrimary,
};

const btn: React.CSSProperties = {
  padding: "8px 14px",
  borderRadius: 4,
  border: `1px solid ${planReviewTheme.btnBorder}`,
  cursor: "pointer",
  fontWeight: 500,
};

const primaryBtn: React.CSSProperties = {
  ...btn,
  background: planReviewTheme.primaryBg,
  borderColor: planReviewTheme.primaryBorder,
  color: "#fff",
};

const primaryBtnDisabled: React.CSSProperties = {
  ...primaryBtn,
  opacity: 0.45,
  cursor: "not-allowed",
};

const dangerBtn: React.CSSProperties = {
  ...btn,
  background: planReviewTheme.dangerBg,
  borderColor: planReviewTheme.dangerBorder,
  color: "#fff",
};

function createDecisionKeyDownHandler(
  activate: () => void,
  decisionLabel: string,
): (e: React.KeyboardEvent<HTMLButtonElement>) => void {
  return (e) => {
    if (e.key !== "Enter" && e.key !== " ") {
      return;
    }
    if (e.repeat) {
      return;
    }
    e.preventDefault();
    if (import.meta.env.DEV) {
      console.debug("[plan-review] Decision keyboard activation", { key: e.key, decision: decisionLabel });
    }
    activate();
  };
}

export function PlanReviewActivityPane({
  prdMarkdown,
  onRefineSubmit,
  onApprove,
  onReject,
  onClientMessage,
}: PlanReviewActivityPaneProps) {
  const refineInputId = useId();
  const [refineDraft, setRefineDraft] = useState("");
  const refineCanSend = refineDraft.trim().length > 0;

  logPlanReviewMarker("M001", "plan_review::PlanReviewActivityPane::render", {
    markdownChars: prdMarkdown.length,
  });
  if (import.meta.env.DEV) {
    console.debug("[plan-review] PlanReviewActivityPane render", {
      markdownChars: prdMarkdown.length,
    });
  }

  const onRefineFormSubmit = useCallback(
    (e: React.FormEvent) => {
      e.preventDefault();
      const text = refineDraft.trim();
      if (import.meta.env.DEV) {
        console.debug("[plan-review] refine form submit", { willSend: text.length > 0 });
      }
      if (!text) {
        return;
      }
      submitPlanRefinement(text, onRefineSubmit, onClientMessage);
      setRefineDraft("");
    },
    [refineDraft, onRefineSubmit, onClientMessage],
  );

  const handleApprove = useCallback(() => {
    if (import.meta.env.DEV) {
      console.debug("[plan-review] Approve control activated");
    }
    submitPlanApprove(onClientMessage);
    onApprove();
  }, [onClientMessage, onApprove]);

  const handleReject = useCallback(() => {
    if (import.meta.env.DEV) {
      console.debug("[plan-review] Reject control activated");
    }
    submitPlanReject(onClientMessage);
    onReject();
  }, [onClientMessage, onReject]);

  const onApproveButtonKeyDown = useCallback(
    createDecisionKeyDownHandler(handleApprove, "approve"),
    [handleApprove],
  );

  const onRejectButtonKeyDown = useCallback(
    createDecisionKeyDownHandler(handleReject, "reject"),
    [handleReject],
  );

  return (
    <div data-testid="session-split-layout" style={shell}>
      <div data-testid="terminal-canvas-region" style={terminalRegion} aria-hidden />
      <div data-testid="plan-activity-pane" style={planPane}>
        <div data-testid="plan-markdown-scroll" style={markdownScroll}>
          <pre style={markdownPre}>{prdMarkdown}</pre>
        </div>
        <form style={refineRow} onSubmit={onRefineFormSubmit}>
          <label htmlFor={refineInputId} style={{ position: "absolute", width: 1, height: 1, overflow: "hidden", clip: "rect(0 0 0 0)" }}>
            Refine plan
          </label>
          <input
            id={refineInputId}
            data-testid="plan-refine-input"
            style={inputStyle}
            value={refineDraft}
            onChange={(ev) => setRefineDraft(ev.target.value)}
            placeholder="Refinement prompt…"
            autoComplete="off"
          />
          <button
            type="submit"
            data-testid="plan-refine-submit"
            style={refineCanSend ? primaryBtn : primaryBtnDisabled}
            disabled={!refineCanSend}
          >
            Send
          </button>
        </form>
        <div data-testid="plan-action-footer" style={footer} role="group" aria-label="Plan decision">
          <button
            type="button"
            data-testid="plan-approve-button"
            style={primaryBtn}
            onClick={handleApprove}
            onKeyDown={onApproveButtonKeyDown}
            aria-label="Approve plan"
          >
            Approve
          </button>
          <button
            type="button"
            data-testid="plan-reject-button"
            style={dangerBtn}
            onClick={handleReject}
            onKeyDown={onRejectButtonKeyDown}
            aria-label="Reject plan"
          >
            Reject
          </button>
        </div>
      </div>
    </div>
  );
}
