# 2026-08-13 — "Base the stack on" in the new-session form, and one-step add-and-start for a planned PR

- **The new-session form can base a PR stack on a session you already have.** Choosing a `pr-stack` recipe now offers a **"Base the stack on"** picker; the orchestrator it creates opens with one planned-PR row already bound to that session's branch, linking to that session.
- **The picker only offers sessions the stack could actually use** — ones that own a branch, live in the project and on the host the form is creating in, and are not already part of another orchestrator's stack. Its default is *"None (agent plans the stack)"*, which is exactly what the form did before.
- **A choice the host refuses is reported in the form**, before anything is created, instead of navigating you to an orchestrator that quietly came up with an empty stack.
- **"+ New planned PR" gained "Add & start session"** — it adds the node and opens its Start-session dialog in one step, pre-filled the same way the row's own button would be, so you no longer hunt for the row you just created.
- The started session is the one you added, even when the orchestrator's agent appended nodes of its own while the form was open: the host names the node it created rather than leaving the panel to guess from the returned plan.
- See [session-drawer.md § Stack Base Session Picker](../session-drawer.md#stack-base-session-picker).
