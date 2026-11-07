# tddy-ui-web

The presentational foundation every web package renders with: the shadcn primitives (`Button`,
`InlineError`, `ScrollArea`, `Select`, `Table`, `Toast`, `Tooltip`), the `cn` class helper, the theme
stylesheet (`tddy-ui-web/theme.css`) and the small hooks and helpers several packages share
(`useIsMobile`, `useVisualViewport`, …).

It imports nothing from `tddy-web`, generated RPC code, ConnectRPC or LiveKit.

Tests: `bun test src` (helpers, boundary rule) and `bun run cypress:component` (primitives).
