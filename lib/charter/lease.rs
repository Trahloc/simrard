use bevy::prelude::*;
use std::any::TypeId;

/// Opaque handle. Only the charter can create these to prevent system forging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LeaseHandle(pub(crate) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkId(pub u32, pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentType {
    Read,
    Write,
}

#[derive(Debug, Clone)]
pub struct LeaseIntent {
    pub reads: Vec<TypeId>,
    pub writes: Vec<TypeId>,
}

#[derive(Component, Debug, Clone)]
pub struct SpatialLease {
    pub primary: ChunkId,
    pub fringe: Vec<ChunkId>,
    pub intent: LeaseIntent,
    pub granted_at_causal_seq: u64,
}

#[derive(Debug, Clone)]
pub enum CharterDenial {
    ChunkConflict {
        contested: Vec<ChunkId>,
        held_by: LeaseHandle,
        retry_after_causal_seq: u64, // The earlist the conflicting lease might expire
    },
    IntentConflict {
        component: TypeId,
        existing_intent: IntentType,
        requested_intent: IntentType,
    },
}

/// Emitted when a lease is granted or denied on a chunk; used by the visualizer to flash the chunk.
/// Bevy 0.18 uses Message + MessageWriter/MessageReader for buffered events.
#[derive(bevy::ecs::message::Message, Debug, Clone)]
pub struct CharterFlashEvent {
    pub chunk: ChunkId,
    pub granted: bool,
}
