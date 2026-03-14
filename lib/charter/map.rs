use crate::lease::{CharterDenial, ChunkId, LeaseHandle, LeaseIntent, SpatialLease};
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Resource, Default)]
pub struct SpatialCharter {
    next_handle_id: u64,
    active: HashMap<ChunkId, Vec<ActiveLease>>,
    /// For watchguard: look up lease by handle to verify declared intent.
    by_handle: HashMap<LeaseHandle, SpatialLease>,
}

#[derive(Debug, Clone)]
pub struct ActiveLease {
    pub handle: LeaseHandle,
    pub lease: SpatialLease,
}

impl SpatialCharter {
    /// O(N) naive implementation for Phase 2 bounds checking.
    pub fn request_lease(&mut self, request: SpatialLease, current_causal_seq: u64) -> Result<LeaseHandle, CharterDenial> {
        let mut all_chunks = request.fringe.clone();
        all_chunks.push(request.primary.clone());

        for chunk in &all_chunks {
            if let Some(leases_in_chunk) = self.active.get(chunk) {
                for active_lease in leases_in_chunk {
                    // Check Intent Compatibility
                    let conflict = self.check_intent_conflict(
                        &active_lease.lease.intent,
                        &request.intent,
                    );

                    if let Some(_denial) = conflict {
                        // We construct a specific denial based on what caused the overlap.
                        // For a simple conflict, we flag the chunk and who holds it.
                        // Phase 2 heuristic: leases might last roughly 5-10 causal steps.
                        return Err(CharterDenial::ChunkConflict {
                            contested: vec![chunk.clone()],
                            held_by: active_lease.handle,
                            retry_after_causal_seq: current_causal_seq + 5, // Simple prediction hint
                        });
                    }
                }
            }
        }

        // Grant the lease if no conflicts
        let new_handle = LeaseHandle(self.next_handle_id);
        self.next_handle_id += 1;

        let active_entry = ActiveLease {
            handle: new_handle,
            lease: request.clone(),
        };
        self.by_handle.insert(new_handle, request);

        for chunk in all_chunks {
            self.active
                .entry(chunk)
                .or_insert_with(Vec::new)
                .push(active_entry.clone());
        }

        Ok(new_handle)
    }

    pub fn release_lease(&mut self, handle: LeaseHandle) {
        self.by_handle.remove(&handle);
        for leases in self.active.values_mut() {
            leases.retain(|active_lease| active_lease.handle != handle);
        }
    }

    /// Look up the lease for a handle (for watchguard verification). Returns None if already released.
    pub fn get_lease(&self, handle: LeaseHandle) -> Option<&SpatialLease> {
        self.by_handle.get(&handle)
    }

    fn check_intent_conflict(&self, existing: &LeaseIntent, requested: &LeaseIntent) -> Option<()> {
        // Write vs Write
        for write_req in &requested.writes {
            if existing.writes.contains(write_req) {
                return Some(());
            }
        }

        // Requested Write vs Existing Read
        for write_req in &requested.writes {
             if existing.reads.contains(write_req) {
                return Some(());
            }
        }

        // Requested Read vs Existing Write
        for read_req in &requested.reads {
             if existing.writes.contains(read_req) {
                return Some(());
            }
        }

        // Read vs Read represents no conflict.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    struct TestComponentA;

    #[test]
    fn test_charter_non_overlapping_chunks() {
        let mut charter = SpatialCharter::default();

        let req1 = SpatialLease {
            primary: ChunkId(0, 0),
            fringe: vec![],
            intent: LeaseIntent { writes: vec![TypeId::of::<TestComponentA>()], reads: vec![] },
            granted_at_causal_seq: 0,
        };
        assert!(charter.request_lease(req1, 1).is_ok());

        let req2 = SpatialLease {
            primary: ChunkId(10, 10), // Far away
            fringe: vec![],
            intent: LeaseIntent { writes: vec![TypeId::of::<TestComponentA>()], reads: vec![] },
            granted_at_causal_seq: 0,
        };
        assert!(charter.request_lease(req2, 1).is_ok(), "Geometrically distant leases should not conflict");
    }

    #[test]
    fn test_charter_read_read_overlap() {
        let mut charter = SpatialCharter::default();

        let req1 = SpatialLease {
            primary: ChunkId(5, 5),
            fringe: vec![],
            intent: LeaseIntent { writes: vec![], reads: vec![TypeId::of::<TestComponentA>()] },
            granted_at_causal_seq: 0,
        };
        assert!(charter.request_lease(req1, 1).is_ok());

        let req2 = SpatialLease {
            primary: ChunkId(5, 5), // Same chunk
            fringe: vec![],
            intent: LeaseIntent { writes: vec![], reads: vec![TypeId::of::<TestComponentA>()] },
            granted_at_causal_seq: 0,
        };
        assert!(charter.request_lease(req2, 1).is_ok(), "Two pawns reading the same chunk/component should not conflict");
    }

    #[test]
    fn test_charter_write_write_conflict() {
        let mut charter = SpatialCharter::default();

        let req1 = SpatialLease {
            primary: ChunkId(5, 5),
            fringe: vec![],
            intent: LeaseIntent { writes: vec![TypeId::of::<TestComponentA>()], reads: vec![] },
            granted_at_causal_seq: 0,
        };
        assert!(charter.request_lease(req1, 1).is_ok());

        let req2 = SpatialLease {
            primary: ChunkId(5, 5), // Same chunk
            fringe: vec![],
            intent: LeaseIntent { writes: vec![TypeId::of::<TestComponentA>()], reads: vec![] },
            granted_at_causal_seq: 0,
        };
        let res = charter.request_lease(req2, 1);
        assert!(res.is_err(), "Write vs Write on the same chunk must conflict");

        if let Err(CharterDenial::ChunkConflict { contested, retry_after_causal_seq, .. }) = res {
            assert_eq!(contested[0], ChunkId(5, 5));
            assert_eq!(retry_after_causal_seq, 6); // 1 + 5 heuristic
        } else {
            panic!("Wrong denial type");
        }
    }

    #[test]
    fn test_exactly_one_pawn_gets_contested_food() {
        // PHASE 2 PROOF:
        // 10 pawns simultaneously request a write lease on the same chunk (food item).
        // No releases in between (true concurrency snapshot).
        // Charter must ensure EXACTLY 1 gets through; all 9 others get ChunkConflict denial.
        let mut charter = SpatialCharter::default();
        let food_chunk = ChunkId(5, 5);
        let causal_seq: u64 = 10;
        let num_pawns = 10;

        let mut granted = 0;
        let mut denied = 0;

        for pawn_id in 0..num_pawns {
            let request = SpatialLease {
                primary: food_chunk.clone(),
                fringe: vec![],
                intent: LeaseIntent {
                    writes: vec![TypeId::of::<TestComponentA>()], // FoodReservation proxy
                    reads: vec![],
                },
                granted_at_causal_seq: causal_seq,
            };

            match charter.request_lease(request, causal_seq) {
                Ok(_handle) => {
                    granted += 1;
                    eprintln!("[causal:{}] Pawn_{} - GRANTED lease on {:?}", causal_seq, pawn_id, food_chunk);
                    // NOTE: We intentionally do NOT release here — proving concurrent exclusion.
                }
                Err(CharterDenial::ChunkConflict { retry_after_causal_seq, .. }) => {
                    denied += 1;
                    eprintln!("[causal:{}] Pawn_{} - DENIED (ChunkConflict, retry after causal:{})", causal_seq, pawn_id, retry_after_causal_seq);
                }
                Err(_) => denied += 1,
            }
        }

        eprintln!("--- Concurrent: {} granted, {} denied out of {} pawns ---", granted, denied, num_pawns);

        // THE KEY INVARIANT: exactly one pawn holds the write lease at a time.
        assert_eq!(granted, 1, "Exactly one pawn must acquire the contested food lease");
        assert_eq!(denied, num_pawns - 1, "All remaining pawns must be denied");

        // Serialized mode: release-on-eat lets all 10 pawns rotate through (queue model).
        eprintln!("\n=== Serialized Turn-Based Access ===");
        let mut charter2 = SpatialCharter::default();
        let mut serialized_total = 0;

        for pawn_id in 0..num_pawns {
            let request = SpatialLease {
                primary: food_chunk.clone(),
                fringe: vec![],
                intent: LeaseIntent { writes: vec![TypeId::of::<TestComponentA>()], reads: vec![] },
                granted_at_causal_seq: causal_seq,
            };
            match charter2.request_lease(request, causal_seq) {
                Ok(handle) => {
                    serialized_total += 1;
                    eprintln!("[serialized] Pawn_{} - GRANTED, eating, releasing", pawn_id);
                    charter2.release_lease(handle); // Simulate eating then vacating
                }
                Err(_) => panic!("Pawn_{} should never be denied in serialized mode because the previous pawn released the slot", pawn_id),
            }
        }

        eprintln!("--- Serialized: all {} pawns took turns ---", serialized_total);
        assert_eq!(serialized_total, num_pawns, "All pawns must thread through in serialized mode");
    }
}
