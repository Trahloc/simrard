pub mod heartbeat;

use bevy::prelude::*;
use simrard_lib_charter::ChunkId;
use simrard_lib_time::CausalClock;
use std::any::TypeId;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::{SystemTime, UNIX_EPOCH};

// ── Drive Type ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveType {
    Hunger,
    Thirst,
    Fatigue,
    Curiosity,
}
// Used by heartbeat and dispatcher for threshold events.

// ── Event Kind ────────────────────────────────────────────────────────────────

/// The set of events the causal queue can carry.
///
/// This enum is an extension point. Future phases and mods add variants here.
/// Order of variants does not imply priority — the queue orders by `causal_seq`.
pub enum CausalEventKind {
    DriveThresholdCrossed { entity: Entity, drive: DriveType },
    LeaseReleased { chunk: ChunkId, component: TypeId },
    ResourceDepleted { chunk: ChunkId },
    // Phase 4+: DiscoveryPropagated, AnsibleBroadcast, etc.
}

// ── Causal Event ──────────────────────────────────────────────────────────────

pub struct CausalEvent {
    /// What happened.
    pub kind: CausalEventKind,
    /// Where it happened (source chunk for propagation).
    pub origin: ChunkId,
    /// The causal_seq at which this event becomes deliverable at its destination.
    /// `origin_seq + propagation_delay(origin, destination, C)`.
    pub deliver_at_causal_seq: u64,
}

/// Newtype for BinaryHeap ordering — we want a min-heap by `deliver_at_causal_seq`.
struct OrderedEvent(CausalEvent);

impl PartialEq for OrderedEvent {
    fn eq(&self, other: &Self) -> bool {
        self.0.deliver_at_causal_seq == other.0.deliver_at_causal_seq
    }
}
impl Eq for OrderedEvent {}

impl PartialOrd for OrderedEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse so BinaryHeap (max-heap) becomes a min-heap by causal_seq
        other.0.deliver_at_causal_seq.cmp(&self.0.deliver_at_causal_seq)
    }
}

// ── Event Queue ───────────────────────────────────────────────────────────────

/// Priority queue of pending causal events ordered by delivery time.
/// Registered as a Bevy `Resource`.
#[derive(Resource, Default)]
pub struct CausalEventQueue {
    heap: BinaryHeap<OrderedEvent>,
}

impl CausalEventQueue {
    /// Enqueue an event to be delivered at `deliver_at_causal_seq`.
    pub fn push(&mut self, event: CausalEvent) {
        self.heap.push(OrderedEvent(event));
    }

    /// Helper: push with delivery time pre-calculated.
    pub fn push_at(&mut self, kind: CausalEventKind, origin: ChunkId, deliver_at: u64) {
        self.push(CausalEvent { kind, origin, deliver_at_causal_seq: deliver_at });
    }

    /// Drain all events ready at `current_seq` (i.e. `deliver_at_causal_seq <= current_seq`).
    /// Boundary condition: events scheduled exactly at `current_seq` ARE included.
    pub fn drain_ready(&mut self, current_seq: u64) -> Vec<CausalEvent> {
        let mut ready = Vec::new();
        while let Some(peeked) = self.heap.peek() {
            if peeked.0.deliver_at_causal_seq <= current_seq {
                ready.push(self.heap.pop().unwrap().0);
            } else {
                break;
            }
        }
        ready
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
}

// ── Propagation Math ──────────────────────────────────────────────────────────

/// Chebyshev (chessboard) distance between two chunks.
/// Diagonal movement costs the same as cardinal — correct for grid worlds.
pub fn chebyshev_distance(a: &ChunkId, b: &ChunkId) -> u32 {
    let dx = (a.0 as i64 - b.0 as i64).unsigned_abs() as u32;
    let dy = (a.1 as i64 - b.1 as i64).unsigned_abs() as u32;
    dx.max(dy)
}

/// How many causal steps until an event at `origin` reaches `target`.
/// C is the propagation constant (chunks per causal step).
/// Same-chunk events deliver in 0 steps.
pub fn propagation_delay(origin: &ChunkId, target: &ChunkId, c: u64) -> u64 {
    let dist = chebyshev_distance(origin, target) as u64;
    dist.div_ceil(c)
}

// ── CausalPropagationClock (moved from time crate; Wolfram-aligned: causality owns propagation) ──

/// Phase 3 clock. Understands spatial propagation — used by the event dispatcher
/// to schedule events at the correct causal_seq for a given destination chunk.
/// C is hardcoded to 8 for Phase 3. Phase 4+ will make it configurable.
#[derive(Resource)]
pub struct CausalPropagationClock {
    pub seq: u64,
    /// Propagation constant: chunks per causal step. Hardcoded to 8 in Phase 3.
    pub c: u64,
}

impl Default for CausalPropagationClock {
    fn default() -> Self {
        Self { seq: 0, c: 8 }
    }
}

impl CausalPropagationClock {
    pub fn increment(&mut self) {
        self.seq += 1;
    }

    /// Steps from `origin` to `target` at propagation speed C.
    pub fn propagation_delay(&self, origin: &ChunkId, target: &ChunkId) -> u64 {
        propagation_delay(origin, target, self.c)
    }
}

impl CausalClock for CausalPropagationClock {
    fn wall_epoch(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
    }

    fn causal_seq(&self) -> u64 {
        self.seq
    }
}

// ── Bevy Plugin ───────────────────────────────────────────────────────────────

pub struct CausalPlugin;

impl Plugin for CausalPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CausalEventQueue>()
            .init_resource::<CausalPropagationClock>();
        // Heartbeat is run from main's sim_tick_driver so sim rate is independent of frame rate.
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── Propagation Delay ─────────────────────────────────────────────────────

    #[test]
    fn test_propagation_delay_cardinal() {
        // Distance 8, C=8 → 1 step (exactly one propagation speed unit)
        assert_eq!(propagation_delay(&ChunkId(0, 0), &ChunkId(8, 0), 8), 1);
    }

    #[test]
    fn test_propagation_delay_two_units() {
        // Distance 16, C=8 → 2 steps
        assert_eq!(propagation_delay(&ChunkId(0, 0), &ChunkId(16, 0), 8), 2);
    }

    #[test]
    fn test_propagation_delay_chebyshev_diagonal() {
        // ChunkId(1,1) is diagonally adjacent: Chebyshev distance = max(1,1) = 1
        // With C=8: ceil(1/8) = 1 step. Proves Chebyshev (not Euclidean ~1.41).
        assert_eq!(propagation_delay(&ChunkId(0, 0), &ChunkId(1, 1), 8), 1);
    }

    #[test]
    fn test_propagation_delay_same_chunk() {
        // Same-chunk events have zero propagation delay — no artificial latency.
        assert_eq!(propagation_delay(&ChunkId(5, 5), &ChunkId(5, 5), 8), 0);
    }

    #[test]
    fn test_propagation_delay_partial_unit() {
        // Distance 3, C=8 → ceil(3/8) = 1 step (not 0). Partial units round up.
        assert_eq!(propagation_delay(&ChunkId(0, 0), &ChunkId(3, 0), 8), 1);
    }

    // ── drain_ready boundary ──────────────────────────────────────────────────

    #[test]
    fn test_drain_ready_boundary_condition() {
        let mut queue = CausalEventQueue::default();

        // Push one event scheduled exactly at seq=15
        queue.push(CausalEvent {
            kind: CausalEventKind::ResourceDepleted { chunk: ChunkId(0, 0) },
            origin: ChunkId(0, 0),
            deliver_at_causal_seq: 15,
        });

        // At seq=14 the event must NOT be present (fencepost: 15 > 14)
        let early = queue.drain_ready(14);
        assert_eq!(early.len(), 0, "Event should not be delivered before its scheduled seq");

        // At seq=15 exactly the event MUST be present (boundary inclusive)
        let on_time = queue.drain_ready(15);
        assert_eq!(on_time.len(), 1, "Event must be delivered at exactly its scheduled seq");
    }

    #[test]
    fn test_drain_ready_multiple_events() {
        let mut queue = CausalEventQueue::default();

        // Three events at different seqs
        for deliver_at in [10u64, 15, 20] {
            queue.push(CausalEvent {
                kind: CausalEventKind::ResourceDepleted { chunk: ChunkId(0, 0) },
                origin: ChunkId(0, 0),
                deliver_at_causal_seq: deliver_at,
            });
        }

        let ready = queue.drain_ready(15);
        assert_eq!(ready.len(), 2, "Events at seq=10 and seq=15 should be ready at current_seq=15");
        assert_eq!(queue.len(), 1, "Event at seq=20 should still be pending");
    }

    // ── Distant pawn decoupling ───────────────────────────────────────────────

    #[test]
    fn test_distant_pawn_no_cross_interaction() {
        // Pawn A in chunk (0,0), Pawn B in chunk (100,100).
        // Event fires at chunk (0,0) at causal_seq=0.
        // Propagation delay to (100,100) with C=8.
        let pawn_a_chunk = ChunkId(0, 0);
        let pawn_b_chunk = ChunkId(100, 100);
        let c = 8u64;

        let delay = propagation_delay(&pawn_a_chunk, &pawn_b_chunk, c);
        // Chebyshev: max(100, 100) = 100. ceil(100/8) = 13 steps.
        assert_eq!(delay, 13);

        // At causal_seq = 12, Pawn B's chunk has NOT received the event.
        // At causal_seq = 13, it has.
        let mut queue = CausalEventQueue::default();
        queue.push(CausalEvent {
            kind: CausalEventKind::ResourceDepleted { chunk: pawn_a_chunk },
            origin: pawn_a_chunk,
            deliver_at_causal_seq: 0 + delay, // deliver at the propagated seq
        });

        assert_eq!(queue.drain_ready(12).len(), 0, "Pawn B must not see the event before propagation reaches its chunk");
        assert_eq!(queue.drain_ready(13).len(), 1, "Pawn B must see the event exactly after propagation delay");
    }
}
