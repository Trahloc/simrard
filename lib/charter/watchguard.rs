use bevy::prelude::*;
use crate::lease::{ChunkId, SpatialLease};
use std::any::TypeId;

/// One write performed under a lease this frame. Lease snapshot taken before release so watchguard can verify after.
#[derive(Debug, Clone)]
pub struct FrameWrite {
    pub actor: Entity,
    pub chunk: ChunkId,
    pub component: TypeId,
    /// Snapshot of the lease at write time (taken before release).
    pub lease_snapshot: Option<SpatialLease>,
}

/// Log of writes performed under lease this frame. Action systems push; watchguard drains and verifies.
#[derive(Resource, Default)]
pub struct FrameWriteLog {
    pub entries: Vec<FrameWrite>,
}

impl FrameWriteLog {
    pub fn log(&mut self, actor: Entity, chunk: ChunkId, component: TypeId, lease_snapshot: Option<SpatialLease>) {
        self.entries.push(FrameWrite { actor, chunk, component, lease_snapshot });
    }
}

#[cfg(debug_assertions)]
pub fn charter_watchguard_system(
    mut log: ResMut<FrameWriteLog>,
) {
    for entry in &log.entries {
        if let Some(ref lease) = entry.lease_snapshot {
            let chunk_ok = lease.primary == entry.chunk || lease.fringe.contains(&entry.chunk);
            let intent_ok = lease.intent.writes.contains(&entry.component);
            if !chunk_ok || !intent_ok {
                eprintln!(
                    "[watchguard] VIOLATION: entity {:?} wrote {:?} at chunk {:?} but lease primary={:?} fringe={:?} writes={:?}",
                    entry.actor,
                    entry.component,
                    entry.chunk,
                    lease.primary,
                    lease.fringe,
                    lease.intent.writes.len()
                );
            }
        } else {
            eprintln!(
                "[watchguard] VIOLATION: entity {:?} wrote at chunk {:?} but no lease snapshot (released before log?)",
                entry.actor, entry.chunk
            );
        }
    }
    log.entries.clear();
}
