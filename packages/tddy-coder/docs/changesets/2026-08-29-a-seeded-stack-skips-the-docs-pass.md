# 2026-08-29 — a seeded stack skips the docs pass

**Type:** Bug Fix

the stack-seeding path records `STATE_STACK_DOCS_WRITTEN` where it recorded `STATE_STACK_PLANNED`, because that state now routes to the new `write-stack-docs` goal and a seeded node is bound to a session whose work already exists, so a retroactive PRD would document a decision nobody is about to make. The two seed acceptance tests already pinned the documented behaviour (a seeded orchestrator comes up in `orchestrate`) and needed no edit.
