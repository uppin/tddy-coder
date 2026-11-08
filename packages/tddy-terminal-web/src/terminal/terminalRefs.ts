import type { MutableRefObject } from "react";
import type { TerminalEngine } from "../engine/TerminalEngine";

/** The terminal container element, set by the component's `ref`. */
export type ContainerRef = { readonly current: HTMLDivElement | null };
/** The live engine, assigned by the lifecycle hook once it is open. */
export type TerminalRef = MutableRefObject<TerminalEngine | null>;
/** The latest `onData` prop, read lazily so listeners need not re-attach on every parent render. */
export type OnDataRef = { readonly current: ((data: string) => void) | undefined };
