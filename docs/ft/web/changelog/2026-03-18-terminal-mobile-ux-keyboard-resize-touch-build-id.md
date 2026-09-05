# 2026-03-18 — Terminal Mobile UX: Keyboard, Resize, Touch, Build ID

- **Keyboard-aware resize**: useVisualViewport hook tracks `visualViewport.height`; terminal container resizes when virtual keyboard opens or closes.
- **Manual keyboard button**: Floating "Keyboard" button at bottom center on mobile; auto-focus disabled; button hides when keyboard open.
- **Focus prevention**: preventFocusOnTap + readonly textarea prevent keyboard from opening on tap; keyboard opens only via Keyboard button.
- **Touch/SGR forwarding**: Capture-phase touch handlers send SGR mouse sequences for interactive TUI (vim, htop); tap-to-click works.
- **Build ID**: Prebuild script generates timestamp; overlay shows build ID for cache verification on mobile.
- **HMR counter**: Dev-only overlay shows hot-reload count when running under Vite.
