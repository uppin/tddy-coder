# The passphrase prompt while a session starts

A start whose clone needs an SSH key passphrase asks for it on the create-session form. Behaviour:
[cloning over SSH](../../../docs/ft/daemon/clone-ssh-passphrase.md). The dialog, its encryption and its
key-continuity check are the Hosts screen's ([the passphrase dialog](hosts-screen.md#the-passphrase-dialog)).

## The pieces

| Piece | Holds |
|---|---|
| `useHostPromptAnswer(instanceId, active, sessionToken)` | the prompt feed for one host, the key-continuity check keyed to the prompt in hand, `answer`, `acceptChangedKey` and `cancel` |
| `HostAddKeyAction` | the Hosts screen's add-key row; renders the dialog from the hook with `active` while its `AddHostKey` call is in flight |
| `StartPassphrasePrompt({ instanceId, active, sessionToken })` | the create-session form's use of the same hook; renders `HostPassphraseDialog` while a prompt is outstanding and nothing otherwise |
| `useHostPromptsFor(hostId, sessionToken)` | the subscription under the hook; `useHostPrompts(hostId)` reads the token from the auth context and calls it |

The hook takes the **session token as an argument**. `CreateSessionPane` is handed its token as a prop and is
mounted without the auth context in many specs, so a hook that read the context would throw there.

## When it listens, and to whom

`CreateSessionPane` mounts `<StartPassphrasePrompt>` beside the start-phase line with `active={submitting}`.
The feed is read only while `active`, so a form nobody has submitted opens no stream, and an `active` that
turns false closes the dialog and forgets the question.

`instanceId` is the host that is cloning: `codebaseDaemonInstanceId` on a split session (the workspace
start is forwarded to the codebase host, which clones), otherwise the session's own host
(`agentHostInstanceId`).

## What the guarantees are

The same as the Hosts screen's, by construction, since it is one hook: the fingerprint on screen is derived
from the key that would do the encrypting; a changed key blocks the answer; the answer is ciphertext the
moment it leaves the dialog; the verdict is held with the prompt it was derived from.

## Testing

`cypress/component/CreateSessionClonePassphraseAcceptance.cy.tsx` (the component: opens on a prompt, opens
no stream while inactive, encrypts under the host's key and sends the operator's token, clears on answer,
cancel and end, blocks on a changed key) and `CreateSessionClonePassphraseMount.cy.tsx` (the real pane,
mounted without the auth context: no feed before a start is submitted, the session's own host for a
co-located start, the codebase host for a split one).
