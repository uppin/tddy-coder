/**
 * The `TerminalEngine` contract suite run against the ghostty-web engine.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-08-terminal-engine-interface.md
 */

import { ghosttyEngine } from "../../src/engine/ghosttyEngine";
import { describeTerminalEngine } from "../../src/engine/terminalEngineContract";

describeTerminalEngine("ghosttyEngine", ghosttyEngine);
