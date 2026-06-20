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

#[cfg(test)]
mod tests {
    use crate::proto::{ActionType, BuildAction, OutputDecl, OutputKind};

    #[test]
    fn action_type_deserializes_each_string_variant() {
        // Given
        let cases = [
            ("command", ActionType::Command),
            ("copy", ActionType::Copy),
            ("tool", ActionType::Tool),
            ("unspecified", ActionType::Unspecified),
        ];
        for (text, expected) in cases {
            // When
            let action: BuildAction =
                serde_json::from_str(&format!(r#"{{"id":"a","type":"{text}"}}"#)).unwrap();

            // Then
            assert_eq!(action.r#type, expected as i32, "type={text}");
        }
    }

    #[test]
    fn action_type_rejects_unknown_string() {
        // Given / When
        let err = serde_json::from_str::<BuildAction>(r#"{"id":"a","type":"bogus"}"#)
            .expect_err("unknown action type must error");

        // Then
        assert!(err.to_string().contains("unknown action type"), "{err}");
    }

    #[test]
    fn output_kind_deserializes_file_directory_and_dir_alias() {
        // When
        let file: OutputDecl = serde_json::from_str(r#"{"path":"p","kind":"file"}"#).unwrap();
        let dir: OutputDecl = serde_json::from_str(r#"{"path":"p","kind":"directory"}"#).unwrap();
        let alias: OutputDecl = serde_json::from_str(r#"{"path":"p","kind":"dir"}"#).unwrap();

        // Then
        assert_eq!(file.kind, OutputKind::File as i32);
        assert_eq!(dir.kind, OutputKind::Directory as i32);
        assert_eq!(alias.kind, OutputKind::Directory as i32);
    }

    #[test]
    fn output_kind_rejects_unknown_string() {
        // Given / When
        let err = serde_json::from_str::<OutputDecl>(r#"{"path":"p","kind":"weird"}"#)
            .expect_err("unknown output kind must error");

        // Then
        assert!(err.to_string().contains("unknown output kind"), "{err}");
    }

    #[test]
    fn action_type_round_trips_through_serialize() {
        // Given
        let action: BuildAction = serde_json::from_str(r#"{"id":"a","type":"copy"}"#).unwrap();

        // When
        let json = serde_json::to_value(&action).unwrap();

        // Then
        assert_eq!(json["type"], "copy");
    }
}
