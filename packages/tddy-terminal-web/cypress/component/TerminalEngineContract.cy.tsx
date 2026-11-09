/**
 * The `TerminalEngine` contract suite run against the ghostty-web engine.
 *
 * PRD: packages/tddy-terminal-web/docs/terminal-engine.md
 */

import { ghosttyEngine } from "../../src/engine/ghosttyEngine";
import { describeTerminalEngine } from "../../src/engine/terminalEngineContract";

describeTerminalEngine("ghosttyEngine", ghosttyEngine);
