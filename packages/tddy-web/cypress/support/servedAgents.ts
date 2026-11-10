/**
 * The spec-derived fields a daemon puts on a `ListAgents` row (`AgentInfo` fields 6–11), per
 * agent id — the fixture mirror of `tddy-daemon-rpc`'s catalog rows, which read them from the
 * agent's YAML spec (`packages/tddy-coding-agent-specs/specs/<id>.yaml`).
 *
 * The web bundle hard-codes no agent: session types, capability gates, permission modes and
 * placements all come from these wire fields. A fixture that omits them serves an agent no
 * daemon would serve, so every mocked agent row spreads {@link servedAgentWire} for its id.
 * An id with no spec (a specialized-agent definition, a stub) gets the empty wire a MODEL row
 * carries.
 */

import type { AgentInfo } from "tddy-client-web/gen/catalog_pb";

/** The metadata fields of one served agent row, keyed by agent id. */
const SERVED_AGENT_WIRE: Record<string, Partial<AgentInfo>> = {
  claude: {
    sessionType: "claude-cli",
    routeProviders: ["anthropic"],
    capabilities: [
      "hooks",
      "managed_workflow",
      "ask_user_question",
      "split_host",
      "project_home_main",
      "requires_model",
      "branch_name_required",
      "operator_notify",
    ],
    runParams: [
      "binary",
      "dangerously_skip_permissions",
      "model",
      "permission_mode",
      "prompt",
      "resume",
      "session_id",
      "slots",
    ],
    permissionModes: ["auto", "default", "acceptEdits", "plan", "bypassPermissions"],
    defaultModel: "opus",
    placements: ["host", "jail"],
  },
  cursor: {
    sessionType: "cursor-cli",
    routeProviders: [],
    capabilities: [
      "hooks",
      "managed_workflow",
      "requires_model",
      "keep_worktree_on_delete",
      "operator_notify",
    ],
    runParams: ["binary", "chat_id", "model", "node_entry", "node_launcher", "prompt", "slots"],
    permissionModes: [],
    defaultModel: "composer-2.5",
    placements: ["host", "jail"],
  },
  codex: {
    sessionType: "codex-cli",
    routeProviders: ["openai_responses"],
    capabilities: [],
    runParams: ["binary"],
    permissionModes: [],
    defaultModel: "gpt-5.3-codex",
    placements: ["host"],
  },
  muse: {
    sessionType: "muse-cli",
    routeProviders: ["meta"],
    capabilities: [],
    runParams: ["binary", "model", "prompt", "resume", "slots"],
    permissionModes: [],
    defaultModel: "muse-spark-1.3",
    placements: ["host", "jail"],
  },
  opencode: {
    sessionType: "opencode-cli",
    routeProviders: ["openai_responses", "openai_chat"],
    capabilities: [],
    runParams: ["binary", "model", "prompt", "resume", "slots"],
    permissionModes: [],
    defaultModel: "",
    placements: ["host", "jail"],
  },
  warp: {
    sessionType: "warp-cli",
    routeProviders: [],
    capabilities: [],
    runParams: ["binary", "resume_token", "slots"],
    permissionModes: [],
    defaultModel: "",
    placements: ["host"],
  },
  oz: {
    sessionType: "oz-cli",
    routeProviders: [],
    capabilities: [],
    runParams: ["binary"],
    permissionModes: [],
    defaultModel: "",
    placements: ["host"],
  },
  "claude-acp": {
    sessionType: "",
    routeProviders: ["anthropic"],
    capabilities: [],
    runParams: ["binary"],
    permissionModes: [],
    defaultModel: "opus",
    placements: ["host"],
  },
  "codex-acp": {
    sessionType: "",
    routeProviders: ["openai_responses"],
    capabilities: [],
    runParams: ["binary"],
    permissionModes: [],
    defaultModel: "gpt-5",
    placements: ["host"],
  },
};

/** The wire fields a daemon serves for agent `id`; empty for an id no spec names. */
export function servedAgentWire(id: string): Partial<AgentInfo> {
  return SERVED_AGENT_WIRE[id] ?? {};
}
