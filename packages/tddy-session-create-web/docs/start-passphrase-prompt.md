# The passphrase prompt while a session starts

A start whose clone needs an SSH key passphrase asks for it on the create-session form. Behaviour:
[cloning over SSH](../../../docs/ft/daemon/clone-ssh-passphrase.md). The dialog, its encryption and its
key-continuity check are the Hosts screen's, which stays in `tddy-web`
([the passphrase dialog](../../tddy-hosts-web/docs/hosts-screen.md#the-passphrase-dialog)) — so this package
holds a **slot**, not the dialog.

## The pieces

| Piece | Where | Holds |
|---|---|---|
| `StartPassphraseProvider({ prompt })` | this package | supplies the component that listens for, and answers, a host's passphrase question |
| `StartPassphrasePrompt({ instanceId, active, sessionToken })` | this package | mounts the provided component; **throws** without a provider, since a clone asking for a passphrase nobody can answer would hang the start with nothing on screen |
| `HostStartPassphrasePrompt` | `tddy-web` `components/hosts/` | the filled slot: `useHostPromptAnswer(instanceId, active, sessionToken)` rendered as `HostPassphraseDialog` while a prompt is outstanding, nothing otherwise |
| `HostStartPassphraseScope` | `tddy-web` `components/hosts/` | `StartPassphraseProvider` with `HostStartPassphrasePrompt`; mounted once at the app root (`index.tsx`) and in `tddy-web`'s Cypress `mount` |

The hook takes the **session token as an argument**. `CreateSessionPane` is handed its token as a prop and is
mounted without the auth context in many specs, so a hook that read the context would throw there.

## When it listens, and to whom

`CreateSessionPane` mounts `<StartPassphrasePrompt>` beside the start progress list with `active={submitting}`.
The feed is read only while `active`, so a form nobody has submitted opens no stream, and an `active` that
turns false closes the dialog and forgets the question.

`instanceId` is the host that is cloning: `codebaseDaemonInstanceId` on a split session (the workspace
start is forwarded to the codebase host, which clones), otherwise the session's own host
(`agentHostInstanceId`).

## Testing

This package's Cypress `mount` provides a prompt that renders nothing, so its specs never meet the throw.
The dialog's behaviour is pinned in `tddy-web`: `CreateSessionClonePassphraseAcceptance.cy.tsx` (opens on a
prompt, opens no stream while inactive, encrypts under the host's key and sends the operator's token, clears
on answer, cancel and end, blocks on a changed key) and `CreateSessionClonePassphraseMount.cy.tsx` (the real
pane, mounted without the auth context: no feed before a start is submitted, the session's own host for a
co-located start, the codebase host for a split one).
