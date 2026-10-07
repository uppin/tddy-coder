# 2026-10-07 — The Screen Sharing tab asks for no passphrase and shows a locked store as locked

**Type:** Feature

`ScreenSharingPassphraseDialog` is deleted and `SessionScreenSharingTab` sends `AddTarget` straight
away. `onListTargets` resolves to `{ targets, vaultLocked }`; the tab shows a `role="status"` notice
(`sessions-screen-sharing-vault-locked`) when the daemon reports the store locked. The tab's reducer
starts with `isVaultLocked: false` — locked is only what `ListTargets` reports. `SessionInspectorDrawer`
drops `onUnlockVault`. `HostPassphraseDialog` is now the only dialog that asks for a screen-sharing
secret. The capability-gating rules are unchanged. Spec:
`cypress/component/SessionInspectorScreenSharingAcceptance.cy.tsx`.
