# 2026-11-08 — Web terminals render with xterm.js

The browser terminal renders with xterm.js: WebGL where the browser offers it, and DOM rendering
otherwise or after the GPU context is lost. Emoji and CJK characters take two cells, as the TUI laid
them out. A program that tracks the mouse receives exactly one report per click and per wheel step.
On mobile, a one-finger drag no longer presses whatever TUI affordance it started on.
