use bevy::prelude::*;
use simrard_lib_pawn::{
    NeuralNetworkComponent, Position, SimulationLogSettings, SimulationReport,
};
use simrard_lib_time::{CausalClock, GlobalTickClock};

use crate::{CausalEventKind, CausalEventQueue, DriveType, propagation_delay};

/// Threshold below which a drive fires a DriveThresholdCrossed event.
const HUNGER_THRESHOLD: f32 = 0.2;
const THIRST_THRESHOLD: f32 = 0.2;
const FATIGUE_THRESHOLD: f32 = 0.2;

/// Drive decay rate per tick (applied every `HEARTBEAT_INTERVAL` ticks).
/// Tuned so pawns can reach food/water and sustain for 10k+ ticks in headless.
const HUNGER_DECAY_PER_TICK: f32 = 0.003;
const THIRST_DECAY_PER_TICK: f32 = 0.0025;
const FATIGUE_DECAY_PER_TICK: f32 = 0.003;

/// How many causal ticks between heartbeat pulses.
/// Drive decay runs every 10 ticks; events fire only when thresholds are crossed.
const HEARTBEAT_INTERVAL: u64 = 10;

/// Propagation speed constant (hardcoded for Phase 3).
pub const C: u64 = 8;

/// One heartbeat pulse: decay and emit threshold events. Call when `seq % HEARTBEAT_INTERVAL == 0`.
/// Used by both the standalone system and the sim tick driver.
pub fn drive_decay_heartbeat_pulse(
    current_seq: u64,
    pawn_query: &mut Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
    event_queue: &mut CausalEventQueue,
    mut report: Option<&mut SimulationReport>,
    stdout_enabled: bool,
) {
    if current_seq % HEARTBEAT_INTERVAL != 0 {
        return;
    }

    for (entity, name, position, mut nn) in pawn_query.iter_mut() {
        // Hunger
        let was_above_h = nn.hunger >= HUNGER_THRESHOLD;
        nn.hunger -= HUNGER_DECAY_PER_TICK * HEARTBEAT_INTERVAL as f32;
        nn.hunger = nn.hunger.max(0.0);
        if was_above_h && nn.hunger < HUNGER_THRESHOLD {
            if let Some(report) = report.as_deref_mut() {
                report.bump("heartbeat_hunger_threshold_crossed");
            }
            let deliver_at = current_seq + propagation_delay(&position.chunk, &position.chunk, C);
            event_queue.push_at(
                CausalEventKind::DriveThresholdCrossed { entity, drive: DriveType::Hunger },
                position.chunk,
                deliver_at,
            );
            if stdout_enabled {
                eprintln!(
                    "[heartbeat:{}] {} hunger crossed threshold ({:.2}) - DriveThresholdCrossed emitted",
                    current_seq, name, nn.hunger
                );
            }
        }

        // Thirst
        let was_above_t = nn.thirst >= THIRST_THRESHOLD;
        nn.thirst -= THIRST_DECAY_PER_TICK * HEARTBEAT_INTERVAL as f32;
        nn.thirst = nn.thirst.max(0.0);
        if was_above_t && nn.thirst < THIRST_THRESHOLD {
            if let Some(report) = report.as_deref_mut() {
                report.bump("heartbeat_thirst_threshold_crossed");
            }
            let deliver_at = current_seq + propagation_delay(&position.chunk, &position.chunk, C);
            event_queue.push_at(
                CausalEventKind::DriveThresholdCrossed { entity, drive: DriveType::Thirst },
                position.chunk,
                deliver_at,
            );
            if stdout_enabled {
                eprintln!(
                    "[heartbeat:{}] {} thirst crossed threshold ({:.2}) - DriveThresholdCrossed emitted",
                    current_seq, name, nn.thirst
                );
            }
        }

        // Fatigue
        let was_above_f = nn.fatigue >= FATIGUE_THRESHOLD;
        nn.fatigue -= FATIGUE_DECAY_PER_TICK * HEARTBEAT_INTERVAL as f32;
        nn.fatigue = nn.fatigue.max(0.0);
        if was_above_f && nn.fatigue < FATIGUE_THRESHOLD {
            if let Some(report) = report.as_deref_mut() {
                report.bump("heartbeat_fatigue_threshold_crossed");
            }
            let deliver_at = current_seq + propagation_delay(&position.chunk, &position.chunk, C);
            event_queue.push_at(
                CausalEventKind::DriveThresholdCrossed { entity, drive: DriveType::Fatigue },
                position.chunk,
                deliver_at,
            );
            if stdout_enabled {
                eprintln!(
                    "[heartbeat:{}] {} fatigue crossed threshold ({:.2}) - DriveThresholdCrossed emitted",
                    current_seq, name, nn.fatigue
                );
            }
        }
    }
}

/// System that runs every frame; only pulses when clock is on an interval.
pub fn drive_decay_heartbeat_system(
    mut pawn_query: Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
    global_clock: Res<GlobalTickClock>,
    mut event_queue: ResMut<CausalEventQueue>,
    mut report: Option<ResMut<SimulationReport>>,
    log_settings: Option<Res<SimulationLogSettings>>,
) {
    drive_decay_heartbeat_pulse(
        global_clock.causal_seq(),
        &mut pawn_query,
        &mut event_queue,
        report.as_deref_mut(),
        log_settings
            .as_deref()
            .map(|settings| settings.stdout_enabled)
            .unwrap_or(true),
    );
}
