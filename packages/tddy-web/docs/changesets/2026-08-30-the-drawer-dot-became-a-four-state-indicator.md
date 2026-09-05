# 2026-08-30 — the drawer dot became a four-state indicator

**Type:** Feature

grey / steady green / blinking green (activity within 30s) / yellow (attention), fed by one daemon-level `StreamSessionNotifications` subscription for the whole drawer; selecting a session settles its dot, a pending elicitation stays yellow until answered, and the fade respects `prefers-reduced-motion`. `SessionIndicatorDot` now renders both the expanded list and the collapsed strip.
