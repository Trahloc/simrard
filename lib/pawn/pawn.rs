use bevy::prelude::*;
use simrard_lib_charter::ChunkId;
use std::collections::BTreeMap;

/// Stable identity for items across the simulation. Used for history and trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemId(pub u64);

/// Global allocator for item IDs. Persists across frames.
#[derive(Resource, Default)]
pub struct ItemIdAllocator {
    next: u64,
}

impl ItemIdAllocator {
    pub fn alloc(&mut self) -> ItemId {
        let id = ItemId(self.next);
        self.next += 1;
        id
    }
}

/// Attached to any entity that represents a distinct "item" (food, water, tool, etc.).
#[derive(Component, Debug, Clone)]
pub struct ItemIdentity {
    pub item_id: ItemId,
    pub created_at_causal_seq: u64,
}

/// One entry: (actor who interacted, action label, causal_seq).
#[derive(Component, Debug, Clone, Default)]
pub struct ItemHistory {
    pub entries: Vec<(Entity, String, u64)>,
}

impl ItemHistory {
    pub fn record(&mut self, actor: Entity, action: impl Into<String>, causal_seq: u64) {
        self.entries.push((actor, action.into(), causal_seq));
    }
}

#[derive(Component, Debug, Clone)]
pub struct Position {
    pub chunk: ChunkId,
}

/// When present, the pawn is trying to reach this chunk.
/// Charter lease is only required at the destination (e.g. for Eat/Drink), not during transit.
#[derive(Component, Debug, Clone, Copy)]
pub struct MovementTarget(pub ChunkId);

#[derive(Component, Debug, Clone)]
pub struct FoodReservation {
    pub portions: u32,
}

/// Water source (e.g. well, pond). Consumed like food; one "portion" satisfies thirst.
#[derive(Component, Debug, Clone)]
pub struct WaterSource {
    pub portions: u32,
}

/// Rest spot (e.g. bed, bench). Pawn with lease can recover fatigue here.
#[derive(Component, Debug, Clone)]
pub struct RestSpot;

#[derive(Component, Debug, Clone)]
pub struct ActiveLeaseHandle(pub simrard_lib_charter::LeaseHandle);

#[derive(Component, Debug, Clone)]
pub struct NeuralNetworkComponent {
    pub hunger: f32,
    pub thirst: f32,
    pub fatigue: f32,
    pub curiosity: f32,
    pub social: f32,
    pub fear: f32,
    pub industriousness: f32,
    pub comfort: f32,
}

impl Default for NeuralNetworkComponent {
    fn default() -> Self {
        Self {
            hunger: 1.0,
            thirst: 1.0,
            fatigue: 1.0,
            curiosity: 1.0,
            social: 1.0,
            fear: 1.0,
            industriousness: 1.0,
            comfort: 1.0,
        }
    }
}

// Global resource to act as an emergent Quest Board
#[derive(Resource, Default)]
pub struct QuestBoard {
    pub active_quests: Vec<Quest>,
}

#[derive(Debug, Clone)]
pub enum QuestStatus {
    Open,
    InProgress { provider: Entity },
    Completed,
}

#[derive(Debug, Clone)]
pub struct Quest {
    pub need: String,
    pub requester: Entity,
    pub chunk: ChunkId,
    pub provider: Option<Entity>,
    pub status: QuestStatus,
}

impl Quest {
    pub fn new(need: impl Into<String>, requester: Entity, chunk: ChunkId) -> Self {
        Self {
            need: need.into(),
            requester,
            chunk,
            provider: None,
            status: QuestStatus::Open,
        }
    }
}

/// What this pawn can do (fulfill). Drives + scorers determine if they *will* do it.
#[derive(Component, Debug, Clone, Default)]
pub struct Capabilities {
    pub can_do: Vec<String>,
}

impl Capabilities {
    pub fn has(&self, capability: &str) -> bool {
        self.can_do.iter().any(|c| c == capability)
    }
}

/// Controls whether simulation systems print per-event lines to stdout/stderr.
#[derive(Resource, Debug, Clone, Copy)]
pub struct SimulationLogSettings {
    pub stdout_enabled: bool,
}

impl Default for SimulationLogSettings {
    fn default() -> Self {
        Self { stdout_enabled: true }
    }
}

/// Aggregated simulation telemetry for headless runs and distilled diagnostics.
#[derive(Resource, Debug, Clone, Default)]
pub struct SimulationReport {
    pub initial_pawn_count: usize,
    pub counters: BTreeMap<&'static str, u64>,
    pub notable_events: Vec<String>,
}

impl SimulationReport {
    const MAX_NOTABLE_EVENTS: usize = 24;

    pub fn set_initial_pawn_count(&mut self, count: usize) {
        self.initial_pawn_count = count;
    }

    pub fn bump(&mut self, key: &'static str) {
        *self.counters.entry(key).or_insert(0) += 1;
    }

    pub fn note(&mut self, message: impl Into<String>) {
        if self.notable_events.len() < Self::MAX_NOTABLE_EVENTS {
            self.notable_events.push(message.into());
        }
    }
}
