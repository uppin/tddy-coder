//! `coding_agent.effort`: the reasoning effort levels a session of the agent can be started at, and
//! how its launch carries the chosen one (`#orchestrators` 3/11).
//!
//! ```yaml
//! effort: { levels: [low, medium, high], default: medium, flag: --effort }
//! effort: { levels: [low, high], env: AGENT_EFFORT }
//! ```
//!
//! The levels are the agent's own words, so the slot is the only place that names them: a picker,
//! the start check and the launch all read it. An agent without the slot takes no effort.

use serde::{Deserialize, Serialize};

/// The `effort` slot, as one launch reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "EffortSlotShape", into = "EffortSlotShape")]
pub struct EffortSlot {
    /// Every level the agent takes, in picker order.
    pub levels: Vec<String>,
    /// The level a picker preselects; `None` leaves the agent's own default (an empty effort).
    pub default: Option<String>,
    /// How a chosen level reaches the agent.
    pub carry: EffortCarry,
}

/// How a chosen effort level reaches the agent's process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffortCarry {
    /// As this flag followed by the level (`--effort high`).
    Flag(String),
    /// As this environment variable set to the level.
    Env(String),
}

/// The slot as it is written: the carry is whichever of `flag` / `env` the file names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EffortSlotShape {
    levels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    flag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    env: Option<String>,
}

impl TryFrom<EffortSlotShape> for EffortSlot {
    type Error = String;

    fn try_from(shape: EffortSlotShape) -> Result<Self, Self::Error> {
        let carry = match (shape.flag, shape.env) {
            (Some(flag), None) => EffortCarry::Flag(flag),
            (None, Some(env)) => EffortCarry::Env(env),
            (Some(_), Some(_)) => {
                return Err(
                    "effort names both a flag and an env: a level is carried one way".to_string(),
                )
            }
            (None, None) => {
                return Err("effort names neither a flag nor an env to carry the level".to_string())
            }
        };
        Ok(Self {
            levels: shape.levels,
            default: shape.default,
            carry,
        })
    }
}

impl From<EffortSlot> for EffortSlotShape {
    fn from(slot: EffortSlot) -> Self {
        let (flag, env) = match slot.carry {
            EffortCarry::Flag(flag) => (Some(flag), None),
            EffortCarry::Env(env) => (None, Some(env)),
        };
        Self {
            levels: slot.levels,
            default: slot.default,
            flag,
            env,
        }
    }
}

/// Why `slot` cannot be loaded: no levels, a level that is blank or named twice, or a `default`
/// that is not one of its levels. `Ok` for a slot a launch can carry. (Naming both a `flag` and an
/// `env`, or neither, is refused as the slot is read.)
pub(crate) fn validate_effort_slot(_slot: &EffortSlot) -> Result<(), String> {
    // TODO(start-effort): implement — refuse an empty list, a blank or repeated level, and a default
    // outside the levels ("default medium is not one of its levels (low, high)").
    Ok(())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    /// The slot written as the YAML flow mapping `yaml`, read and then validated as a load does.
    fn a_loaded_slot(yaml: &str) -> Result<EffortSlot, String> {
        let slot: EffortSlot = serde_yaml::from_str(yaml).map_err(|e| e.to_string())?;
        validate_effort_slot(&slot)?;
        Ok(slot)
    }

    #[test]
    fn a_slot_carried_by_an_env_variable_reads_as_that_variable() {
        // When
        let slot = a_loaded_slot("{ levels: [low, high], default: low, env: LARK_EFFORT }");

        // Then — guard: the shape is read today
        assert_eq!(
            slot,
            Ok(EffortSlot {
                levels: vec!["low".to_string(), "high".to_string()],
                default: Some("low".to_string()),
                carry: EffortCarry::Env("LARK_EFFORT".to_string()),
            })
        );
    }

    #[test]
    fn a_slot_naming_both_a_flag_and_an_env_is_refused() {
        // When
        let refused = a_loaded_slot("{ levels: [low], flag: --effort, env: LARK_EFFORT }");

        // Then — guard: refused as the slot is read
        assert!(
            refused
                .as_ref()
                .is_err_and(|message| message.contains("both a flag and an env")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_slot_with_no_levels_is_refused() {
        // When
        let refused = a_loaded_slot("{ levels: [], flag: --effort }");

        // Then
        assert_eq!(refused, Err("declares no levels".to_string()));
    }

    #[test]
    fn a_slot_naming_a_level_twice_is_refused_naming_it() {
        // When
        let refused = a_loaded_slot("{ levels: [low, high, low], flag: --effort }");

        // Then
        assert_eq!(refused, Err("level low is named twice".to_string()));
    }

    #[test]
    fn a_slot_with_a_blank_level_is_refused() {
        // When
        let refused = a_loaded_slot("{ levels: [low, \" \"], flag: --effort }");

        // Then
        assert_eq!(refused, Err("a level is blank".to_string()));
    }

    #[test]
    fn a_default_that_is_not_one_of_the_levels_is_refused_naming_both() {
        // When
        let refused = a_loaded_slot("{ levels: [low, high], default: medium, flag: --effort }");

        // Then
        assert_eq!(
            refused,
            Err("default medium is not one of its levels (low, high)".to_string())
        );
    }
}
