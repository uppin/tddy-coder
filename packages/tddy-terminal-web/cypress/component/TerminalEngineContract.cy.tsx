/**
 * The `TerminalEngine` contract suite run against every engine.
 *
 * PRD: packages/tddy-terminal-web/docs/terminal-engine.md,
 * docs/ft/web/1-WIP/PRD-2026-11-08-xterm-terminal-engine.md (acceptance criterion 2)
 */

import { ghosttyEngine } from "../../src/engine/ghosttyEngine";
import { xtermEngine } from "../../src/engine/xtermEngine";
import { describeTerminalEngine } from "../../src/engine/terminalEngineContract";

describeTerminalEngine("ghosttyEngine", ghosttyEngine);
describeTerminalEngine("xtermEngine", xtermEngine);
