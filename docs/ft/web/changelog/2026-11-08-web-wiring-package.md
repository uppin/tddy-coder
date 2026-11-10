# 2026-11-08 — Web wiring package

`tddy-web` now only composes the web packages through an app container. Fixes styling the package split had dropped: classes used only inside a component package (the host
desktop overlay's dark backdrop, the code hover card's height bound and others) are back in the app's
stylesheet. See
[web-wiring-package.md](../web-wiring-package.md).
