/**
 * Appended after Electrobun’s built-in preload (second `initWebview` preload arg).
 * Default preload only emits `new-window-open` on cmd/ctrl+click, not on plain
 * `target="_blank"` — so OAuth links were no-ops. Mirror cmd+click: post the same
 * webviewEvent shape to the event bridge.
 */
declare const window: Window & {
  __electrobunWebviewId?: number;
  __electrobunEventBridge?: { postMessage: (msg: string) => void };
  __electrobunInternalBridge?: { postMessage: (msg: string) => void };
};

(function tddyOpenExternalBlankTargets(): void {
  window.addEventListener(
    "click",
    (event: Event) => {
      const e = event as MouseEvent;
      const t = e.target;
      if (!t || typeof (t as Node).nodeType !== "number") {
        return;
      }
      const anchor = (t as HTMLElement).closest?.("a");
      if (!anchor) {
        return;
      }
      const a = anchor as HTMLAnchorElement;
      const href = a.href;
      if (!href) {
        return;
      }
      const lower = href.toLowerCase();
      if (!lower.startsWith("http://") && !lower.startsWith("https://")) {
        return;
      }
      const target = (a.getAttribute("target") || "").toLowerCase();
      if (target !== "_blank" && target !== "_new") {
        return;
      }
      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation();
      const detail = JSON.stringify({
        url: href,
        isCmdClick: false,
        tddyTargetBlank: true,
      });
      setTimeout(() => {
        const bridge =
          window.__electrobunEventBridge || window.__electrobunInternalBridge;
        bridge?.postMessage(
          JSON.stringify({
            id: "webviewEvent",
            type: "message",
            payload: {
              id: window.__electrobunWebviewId,
              eventName: "new-window-open",
              detail,
            },
          }),
        );
      });
    },
    true,
  );
})();
