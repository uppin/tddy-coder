import "./index.css";
import { createRoot } from "react-dom/client";
import { applyDebugMaskFromUrl } from "tddy-client-web";
import { appHandler, composeApp } from "./app/composeApp";

// Honour `?debug=` immediately (before terminals mount); `/api/config` re-syncs afterwards.
applyDebugMaskFromUrl();

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(composeApp(appHandler({ window: typeof window === "undefined" ? undefined : window })));
}
