import type { MutableRefObject } from "react";
import type { Terminal } from "ghostty-web";

/** The terminal container element, set by the component's `ref`. */
export type ContainerRef = { readonly current: HTMLDivElement | null };
/** The live emulator, assigned by the lifecycle hook once it is open. */
export type TerminalRef = MutableRefObject<Terminal | null>;
/** The latest `onData` prop, read lazily so listeners need not re-attach on every parent render. */
export type OnDataRef = { readonly current: ((data: string) => void) | undefined };
