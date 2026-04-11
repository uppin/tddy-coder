# `CodexOAuthDialog`

## Overview

**`CodexOAuthDialog`** is a modal for the Codex CLI OAuth authorize step. It renders when **`open`** is true; when **`open`** is false the component returns **`null`** (no DOM subtree).

## Props

| Prop | Type | Role |
|------|------|------|
| **`authorizeUrl`** | **`string \| null`** | HTTPS authorize URL for the iframe or external link. |
| **`open`** | **`boolean`** | Controls visibility; when false, nothing is rendered. |
| **`onDismiss`** | **`() => void`** | Called when the user dismisses the dialog (**`data-testid="codex-oauth-dismiss"`**). |
| **`embeddingBlocked`** | **`boolean`** (optional) | When true, the iframe is omitted and the **embedding-blocked** panel is shown instead. |

## Layout and test hooks

- Root overlay: **`data-testid="codex-oauth-dialog"`**, **`role="dialog"`**, **`aria-modal="true"`**.
- Dismiss control: **`data-testid="codex-oauth-dismiss"`**.
- Embedding-blocked panel: **`data-testid="codex-oauth-embedding-fallback"`** (copy + link with **`target="_blank"`** and **`rel="noopener noreferrer"`**).
- Normal path: sandboxed **iframe** (**`sandbox="allow-forms allow-scripts allow-same-origin allow-popups"`**) when **`embeddingBlocked`** is false and **`authorizeUrl`** is set.

## Tests

Component specs: **`cypress/component/CodexOAuthDialog.cy.tsx`**, **`cypress/component/CodexOAuthIframeFallback.cy.tsx`**.

```bash
bunx cypress run --component --spec "cypress/component/CodexOAuth*.cy.tsx"
```

## Feature documentation

**[Codex OAuth web relay](../../../../docs/ft/web/codex-oauth-web-relay.md)**
