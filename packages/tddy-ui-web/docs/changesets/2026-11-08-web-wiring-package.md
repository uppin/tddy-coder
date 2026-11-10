# 2026-11-08 — The theme scans each component package by name (#web-split 20/20, PR #853)

`theme.css` named the component packages as `@source "../../tddy-*-web/src"`. Tailwind does not
expand a wildcard in a directory name there, so it scanned none of them, and every class used only
inside a component package was missing from the app's stylesheet (and the Storybook's) from #840 on —
the host-desktop overlay's `bg-black/90`, the code hover card's `max-h-80` among them; the built CSS
grows from 37 kB to 57 kB with the fix. The theme now names each package with its own `@source` line,
and `src/themeSources.test.ts` pins that: no wildcard, and every package that renders Tailwind classes
is listed.
#861 carried an interim glob fix (`@source "../../tddy-*-web/src/**/*.{ts,tsx}"`) for its CI; this
explicit list replaces it when #853 is rebased onto the fixed chain.
