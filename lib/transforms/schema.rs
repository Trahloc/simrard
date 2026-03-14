use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Operation {
    Modify,      // relative multiplier or add
    AddField,
    Deprecate,   // soft removes a field but leaves a tombstone
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transform {
    pub target: String,          // e.g. "pawn.needs" or "entity.*"
    pub operation: Operation,
    pub path: String,            // dot-path: "hunger.depletion_rate"

    #[serde(default)]
    pub value: String,           // e.g. "0.8x" or "12.5"

    pub contract: Option<String>,// e.g. ">=epoch:2847" // Note: This refers to causal epoch, not wall time
    pub epoch: u64,              // monotonic causal ID
    pub author: String,          // for mod CI

    #[serde(default)]
    pub alias: Option<String>,   // e.g. If renaming a field, what was its old name?
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransformStack {
    pub base_epoch: u64,
    pub transforms: Vec<Transform>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnostic {
    SameFieldConflict { path: String, epoch_a: u64, epoch_b: u64 },
    FieldTombstoned { path: String, retired_at: u64, successor: Option<String> },
    MissingField { path: String, transform_id: String }, // transform_id could be author:epoch
}
