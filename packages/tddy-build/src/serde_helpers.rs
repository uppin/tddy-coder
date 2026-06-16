//! String ↔ prost enum (`i32`) serde converters.
//!
//! prost generates proto enums as `i32` fields, but `BUILD.yaml` authors them as
//! snake_case strings (e.g. `type: command`, `kind: file`). These converters are
//! wired into the generated types via `field_attribute` in `build.rs`.

use serde::{Deserialize, Deserializer, Serializer};

use crate::proto::{ActionType, OutputKind};

pub fn serialize_action_type<S>(value: &i32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let name = match ActionType::try_from(*value) {
        Ok(ActionType::Command) => "command",
        Ok(ActionType::Copy) => "copy",
        Ok(ActionType::Tool) => "tool",
        _ => "unspecified",
    };
    serializer.serialize_str(name)
}

pub fn deserialize_action_type<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    let value = match raw.as_str() {
        "" | "unspecified" => ActionType::Unspecified,
        "command" => ActionType::Command,
        "copy" => ActionType::Copy,
        "tool" => ActionType::Tool,
        other => {
            return Err(serde::de::Error::custom(format!(
                "unknown action type: {other:?} (expected command|copy|tool)"
            )))
        }
    };
    Ok(value as i32)
}

pub fn serialize_output_kind<S>(value: &i32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let name = match OutputKind::try_from(*value) {
        Ok(OutputKind::File) => "file",
        Ok(OutputKind::Directory) => "directory",
        _ => "unspecified",
    };
    serializer.serialize_str(name)
}

pub fn deserialize_output_kind<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    let value = match raw.as_str() {
        "" | "unspecified" => OutputKind::Unspecified,
        "file" => OutputKind::File,
        "directory" | "dir" => OutputKind::Directory,
        other => {
            return Err(serde::de::Error::custom(format!(
                "unknown output kind: {other:?} (expected file|directory)"
            )))
        }
    };
    Ok(value as i32)
}
