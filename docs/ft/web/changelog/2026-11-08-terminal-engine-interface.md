# 2026-11-08 — Terminal engine interface

No change to what the terminal does. The browser terminal talks to its emulator through one
interface with top-based scroll positions, so the emulator library can be replaced without changing
scrolling, history paging, zoom or input handling.
