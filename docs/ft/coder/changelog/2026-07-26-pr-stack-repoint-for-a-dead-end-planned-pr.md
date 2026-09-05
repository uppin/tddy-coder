# 2026-07-26 — PR-Stack repoint for a dead-end planned PR

- A planned PR whose predecessor's PR merged and whose branch was then deleted on `origin` is no longer a dead end: the row offers **"Repoint to `<branch>`"**, naming where the node will land before you click.
- Repoint is offered for **any** base that cannot be resolved right now, not only a predecessor the plan records as merged — that field is written by the orchestrator agent and reads `open` if you merged on GitHub without running an assess pass.
- A blocked row no longer replaces itself with an error. It keeps its title, description, planned branch and the new **base branch** line, with **Start session** disabled and a warning naming each blocking issue.
- Repointing **collapses the node onto a single predecessor** — the one owning the target — or detaches it onto the default branch when none survives. A node that was never started is repointed as a plan-only edit.
- A refused or failed repoint now shows the daemon's reason on the row and leaves it blocked, instead of appearing to do nothing.
- A **root** node's Start-session dialog finally names its base branch instead of reading "New branch from base:" with no name.
