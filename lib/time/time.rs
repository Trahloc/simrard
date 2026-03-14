use bevy::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// Causal clock interface providing the distinction between
/// external human wall-time history and internal monotonic simulation sequences.
pub trait CausalClock {
    /// Unix timestamp in seconds. Used for provenance, logging, saves, and mod history.
    fn wall_epoch(&self) -> u64;

    /// Monotonic internal counter. Used for strict internal ordering and contract validation.
    fn causal_seq(&self) -> u64;
}

// ── GlobalTickClock ───────────────────────────────────────────────────────────

/// Heartbeat clock. Its sole remaining role in Phase 3+ is drive decay and
/// environmental simulation — periodic updates that don't depend on spatial events.
/// Pawn *decisions* are now driven by `CausalEventQueue`, not this clock.
#[derive(Resource)]
pub struct GlobalTickClock {
    /// Total causal ticks passed since simulation start
    seq: u64,
}

impl Default for GlobalTickClock {
    fn default() -> Self {
        Self { seq: 0 }
    }
}

impl GlobalTickClock {
    pub fn increment(&mut self) {
        self.seq += 1;
    }
}

impl CausalClock for GlobalTickClock {
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

// ── Sim time scale (observer mode) ─────────────────────────────────────────────

/// Sim speed multiplier. 1.0 = normal (e.g. 10 causal ticks/sec), 2.0 = double, 0.5 = half.
#[derive(Resource)]
pub struct SimTimeScale(pub f32);

impl Default for SimTimeScale {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Accumulated wall time toward the next sim tick. Driver consumes this.
#[derive(Resource)]
pub struct SimTickAccumulator(pub f32);

impl Default for SimTickAccumulator {
    fn default() -> Self {
        Self(0.0)
    }
}

/// Causal ticks per second when scale is 1.0. "Normal" observer speed.
pub const SIM_TICKS_PER_SECOND_AT_1X: f32 = 10.0;

// ── TimePlugin ────────────────────────────────────────────────────────────────

pub struct TimePlugin;

impl Plugin for TimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GlobalTickClock>()
            .init_resource::<SimTimeScale>()
            .init_resource::<SimTickAccumulator>();
    }
}
