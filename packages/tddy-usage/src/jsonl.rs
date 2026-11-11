//! The `jsonl` format: one conversation's lines, filtered, deduplicated and aggregated.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;

use crate::{Aggregate, FieldPath, RecordSpec};

/// What a conversation's files add up to under a [`RecordSpec`].
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Folded {
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
    pub reasoning: u64,
    pub cost: Option<f64>,
    /// The last model seen, if any line named one.
    pub model: Option<String>,
    /// The number of kept records.
    pub turns: u32,
}

/// One kept record: the figures read from a line.
#[derive(Default)]
struct Reading {
    counters: [u64; 5],
    cost: Option<f64>,
}

/// Fold every line of `files` under `spec`: lines that are not JSON or fail a `where` predicate are
/// dropped, the last line per `dedupe_by` key wins (a line without the key is a record of its own),
/// the counters are summed, `model` is the last value seen and `turns` counts the kept records.
/// An unreadable file contributes nothing.
pub(crate) fn fold_files(files: &[PathBuf], spec: &RecordSpec) -> Folded {
    let mut records: Vec<Reading> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    let mut model = None;

    for file in files {
        let text = match std::fs::read_to_string(file) {
            Ok(text) => text,
            Err(e) => {
                log::warn!("usage: cannot read {}: {e}", file.display());
                continue;
            }
        };
        for line in text.lines() {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if !passes_where(&value, spec) {
                continue;
            }
            if let Some(name) = spec
                .model
                .as_ref()
                .and_then(|path| path.lookup(&value))
                .and_then(Value::as_str)
            {
                model = Some(name.to_string());
            }
            let reading = read_line(&value, spec);
            let key = spec
                .dedupe_by
                .as_ref()
                .and_then(|path| path.lookup(&value))
                .map(Value::to_string);
            keep(&mut records, &mut by_key, key, reading);
        }
    }

    sum(&records, model, spec.aggregate)
}

/// Keep `reading`: in place of the earlier record with the same dedupe `key`, else as a new one.
fn keep(
    records: &mut Vec<Reading>,
    by_key: &mut HashMap<String, usize>,
    key: Option<String>,
    reading: Reading,
) {
    let Some(key) = key else {
        records.push(reading);
        return;
    };
    match by_key.get(&key) {
        Some(&index) => records[index] = reading,
        None => {
            by_key.insert(key, records.len());
            records.push(reading);
        }
    }
}

/// Whether `value` satisfies every `where` predicate of `spec`.
fn passes_where(value: &Value, spec: &RecordSpec) -> bool {
    spec.where_.iter().all(|predicate| {
        predicate
            .field
            .lookup(value)
            .and_then(Value::as_str)
            .is_some_and(|actual| actual == predicate.equals)
    })
}

/// The kept records folded by `aggregate`: each counter and the cost summed.
fn sum(records: &[Reading], model: Option<String>, aggregate: Aggregate) -> Folded {
    let Aggregate::Sum = aggregate;
    let mut folded = Folded {
        model,
        turns: records.len() as u32,
        ..Folded::default()
    };
    for record in records {
        let [input, cache_read, cache_write, output, reasoning] = record.counters;
        folded.input += input;
        folded.cache_read += cache_read;
        folded.cache_write += cache_write;
        folded.output += output;
        folded.reasoning += reasoning;
        if let Some(cost) = record.cost {
            *folded.cost.get_or_insert(0.0) += cost;
        }
    }
    folded
}

/// The counters and cost one line reports; a field that is missing or not a number reads as zero.
fn read_line(value: &Value, spec: &RecordSpec) -> Reading {
    let number = |path: &Option<FieldPath>| {
        path.as_ref()
            .and_then(|p| p.lookup(value))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    let tokens = &spec.tokens;
    Reading {
        counters: [
            number(&tokens.input),
            number(&tokens.cache_read),
            number(&tokens.cache_write),
            number(&tokens.output),
            number(&tokens.reasoning),
        ],
        cost: spec
            .cost
            .as_ref()
            .and_then(|p| p.lookup(value))
            .and_then(Value::as_f64),
    }
}
