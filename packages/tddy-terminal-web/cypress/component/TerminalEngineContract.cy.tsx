/**
 * The `TerminalEngine` contract suite run against every engine.
 *
 * Docs: packages/tddy-terminal-web/docs/terminal-engine.md
 */

import { xtermEngine } from "../../src/engine/xtermEngine";
import { describeTerminalEngine } from "../../src/engine/terminalEngineContract";

describeTerminalEngine("xtermEngine", xtermEngine);
