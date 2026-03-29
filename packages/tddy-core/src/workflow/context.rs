//! Context — typed key-value store for workflow state.
//!
//! Mirrors [graph-flow context.rs](https://github.com/a-agmon/rs-graph-llm/blob/main/graph-flow/src/context.rs).
//! Must implement Serialize/Deserialize for filesystem persistence.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Thread-safe typed key-value store for workflow runtime state.
#[derive(Clone, Debug, Default)]
pub struct Context {
    inner: Arc<DashMap<String, serde_json::Value>>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    pub async fn get<T>(&self, key: &str) -> Option<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.inner
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn get_sync<T>(&self, key: &str) -> Option<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.inner
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub async fn set<T>(&self, key: &str, value: T)
    where
        T: Serialize,
    {
        if let Ok(v) = serde_json::to_value(value) {
            self.inner.insert(key.to_string(), v);
        }
    }

    pub fn set_sync<T>(&self, key: &str, value: T)
    where
        T: Serialize,
    {
        if let Ok(v) = serde_json::to_value(value) {
            self.inner.insert(key.to_string(), v);
        }
    }

    /// Remove a key. Use when clearing task-scoped state (e.g. answers) before the next task.
    pub fn remove_sync(&self, key: &str) {
        self.inner.remove(key);
    }

    /// Merge top-level JSON object entries into this context (tool-driven session updates).
    /// Existing keys are overwritten when the patch contains the same key.
    pub fn merge_json_object_sync(&self, map: &serde_json::Map<String, serde_json::Value>) {
        log::debug!(
            target: "tddy_core::workflow::context",
            "merge_json_object_sync: merging {} key(s)",
            map.len()
        );
        for (k, v) in map {
            log::debug!(
                target: "tddy_core::workflow::context",
                "merge_json_object_sync: set key={} value_type={}",
                k,
                if v.is_null() {
                    "null"
                } else if v.is_object() {
                    "object"
                } else if v.is_array() {
                    "array"
                } else {
                    "scalar"
                }
            );
            self.inner.insert(k.clone(), v.clone());
        }
    }
}

impl Serialize for Context {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.inner.len()))?;
        for ref_multi in self.inner.iter() {
            map.serialize_entry(ref_multi.key(), ref_multi.value())?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Context {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map: std::collections::HashMap<String, serde_json::Value> =
            Deserialize::deserialize(deserializer)?;
        let inner = Arc::new(DashMap::new());
        for (k, v) in map {
            inner.insert(k, v);
        }
        Ok(Self { inner })
    }
}

#[cfg(test)]
mod merge_json_tests {
    use super::Context;
    use serde_json::json;

    #[test]
    fn merge_json_object_sync_inserts_keys() {
        let ctx = Context::new();
        let mut map = serde_json::Map::new();
        map.insert("run_optional_step_x".to_string(), json!(true));
        ctx.merge_json_object_sync(&map);
        assert_eq!(
            ctx.get_sync::<bool>("run_optional_step_x"),
            Some(true),
            "merge_json_object_sync must persist keys into context"
        );
    }
}
