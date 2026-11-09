/**
 * The slot where the passphrase question a host raises while a session start is cloning is shown
 * over the create-session form.
 *
 * A start that must clone its project blocks on the host's ssh key passphrase, and the form is the
 * only surface the operator is looking at. The question, its key-continuity check and its answer
 * belong to the Hosts screen, so this package does not own them: the host application provides a
 * component through {@link StartPassphraseProvider}, and the form mounts it while a start is in
 * flight against the host that is cloning.
 *
 * Feature: docs/ft/daemon/clone-ssh-passphrase.md
 */

import React, { createContext, useContext, type ComponentType } from "react";

export interface StartPassphrasePromptProps {
  /** The daemon that is cloning — the one whose prompt feed is read and whose prompts are answered. */
  instanceId: string;
  /** Whether a start is in flight. */
  active: boolean;
  /** The operator's session — the one the start was made with, so the prompt is theirs to answer. */
  sessionToken: string;
}

const StartPassphraseContext = createContext<ComponentType<StartPassphrasePromptProps> | null>(null);

/** Supplies the component that listens for, and answers, a host's passphrase question. */
export function StartPassphraseProvider({
  prompt,
  children,
}: {
  prompt: ComponentType<StartPassphrasePromptProps>;
  children?: React.ReactNode;
}): React.ReactElement {
  return <StartPassphraseContext.Provider value={prompt}>{children}</StartPassphraseContext.Provider>;
}

/**
 * Mounts the provided passphrase prompt. Throws without a {@link StartPassphraseProvider}: a clone
 * that asks for a passphrase nobody can answer would otherwise hang the start with nothing on screen.
 */
export function StartPassphrasePrompt(props: StartPassphrasePromptProps): React.ReactElement {
  const Prompt = useContext(StartPassphraseContext);
  if (Prompt === null) {
    throw new Error("StartPassphrasePrompt needs a StartPassphraseProvider above it");
  }
  return <Prompt {...props} />;
}
