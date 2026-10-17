# 2026-10-18 — "Create Remote Branch" is unchecked by default

- The Start-Session dialog's **"Create Remote Branch"** toggle now starts **unchecked**, so an ad-hoc session no longer pushes its new branch to `origin` — and no longer waits on the repository's `pre-push` hooks — unless asked to. Starting a **PR-stack node** still pre-checks it, because a node's branch is the base its successors' PRs target. See [session-drawer.md § Create Session](../session-drawer.md#create-session).
