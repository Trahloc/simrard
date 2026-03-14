use bevy::prelude::*;
use bevy::ecs::message::MessageWriter;
use simrard_lib_utility_ai::prelude::*;
use simrard_lib_causal::{CausalEventKind, CausalEventQueue, DriveType};
use simrard_lib_charter::{
    CharterDenial, CharterFlashEvent, ChunkId, FrameWriteLog, LeaseIntent, SpatialCharter,
    SpatialLease,
};
use simrard_lib_pawn::{
    ActiveLeaseHandle, FoodReservation, ItemHistory, MovementTarget, NeuralNetworkComponent,
    Position, QuestBoard, Quest, QuestStatus, RestSpot, SimulationLogSettings,
    SimulationReport, WaterSource,
};
use simrard_lib_time::{CausalClock, GlobalTickClock};
use std::any::TypeId;
use std::collections::VecDeque;

fn stdout_enabled(settings: Option<&SimulationLogSettings>) -> bool {
    settings.map(|settings| settings.stdout_enabled).unwrap_or(true)
}

/// Optional UI feed: when present, action systems and dispatcher push short activity strings.
/// Bin inits this and displays the last N lines.
#[derive(Resource, Default)]
pub struct ActivityLog(pub VecDeque<String>);

const ACTIVITY_LOG_MAX: usize = 32;

impl ActivityLog {
    pub fn push(&mut self, s: String) {
        self.0.push_back(s);
        while self.0.len() > ACTIVITY_LOG_MAX {
            self.0.pop_front();
        }
    }
}

#[derive(Component, Debug, Clone, ScorerBuilder)]
pub struct HungerScorer;
#[derive(Component, Debug, Clone, ScorerBuilder)]
pub struct ThirstScorer;
#[derive(Component, Debug, Clone, ScorerBuilder)]
pub struct FatigueScorer;

#[derive(Component, Debug, Clone, ActionBuilder)]
pub struct EatAction;
#[derive(Component, Debug, Clone, ActionBuilder)]
pub struct DrinkAction;
#[derive(Component, Debug, Clone, ActionBuilder)]
pub struct RestAction;

#[derive(Component, Debug, Clone, ScorerBuilder)]
pub struct NeedsToMoveScorer;

#[derive(Component, Debug, Clone, ActionBuilder)]
pub struct MoveToChunkAction;

/// Chunk grid extent (0..=CHUNK_EXTENT). Matches bin visualizer; movement is clamped to this range.
const CHUNK_EXTENT: u32 = 11;

pub struct PawnAIPlugin;

impl Plugin for PawnAIPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            (
                needs_to_move_scorer_system,
                hunger_scorer_system,
                thirst_scorer_system,
                fatigue_scorer_system,
            )
                .in_set(BigBrainSet::Scorers),
        );
        app.add_systems(
            PreUpdate,
            (
                move_to_chunk_action_system,
                eat_action_system,
                drink_action_system,
                rest_action_system,
            )
                .in_set(BigBrainSet::Actions),
        );
        // Dispatcher and death check are run from main's sim_tick_driver / after it.
    }
}

/// Pawns die when hunger or thirst reaches 0. Run *before* sim_tick_driver so no system
/// (dispatcher, BigBrain) ever sees or queues commands for dead entities.
/// Death is failure: pawns must move to food/water or they die.
pub fn pawn_death_system(
    mut commands: Commands,
    query: Query<(Entity, &Name, &NeuralNetworkComponent)>,
    mut activity: Option<ResMut<ActivityLog>>,
    mut quest_board: Option<ResMut<QuestBoard>>,
    global_clock: Option<Res<GlobalTickClock>>,
    mut report: Option<ResMut<SimulationReport>>,
) {
    let to_despawn: Vec<(Entity, String)> = query
        .iter()
        .filter(|(_, _, nn)| nn.hunger <= 0.0 || nn.thirst <= 0.0)
        .map(|(e, name, _)| (e, format!("{}", name)))
        .collect();
    let dead: std::collections::HashSet<_> = to_despawn.iter().map(|(e, _)| *e).collect();
    if let Some(ref mut board) = quest_board {
        board.active_quests.retain(|q| !dead.contains(&q.requester));
    }
    for (entity, name) in to_despawn {
        if let Some(ref mut log) = activity {
            log.push(format!("{} died (hunger/thirst zero)", name));
        }
        if let Some(ref mut report) = report {
            report.bump("pawn_deaths");
            let tick = global_clock
                .as_deref()
                .map(CausalClock::causal_seq)
                .unwrap_or_default();
            report.note(format!("tick {}: {} died", tick, name));
        }
        commands.entity(entity).despawn();
    }
}

/// One dispatcher step: drain events ready at `current_seq` and apply them.
/// Used by both the standalone system and the sim tick driver.
pub fn pawn_event_dispatcher_step(
    current_seq: u64,
    event_queue: &mut CausalEventQueue,
    quest_board: &mut QuestBoard,
    pawn_query: &mut Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
    mut activity: Option<&mut ActivityLog>,
    mut report: Option<&mut SimulationReport>,
    stdout_enabled: bool,
) {
    let ready_events = event_queue.drain_ready(current_seq);

    for event in ready_events {
        match event.kind {
            CausalEventKind::DriveThresholdCrossed { entity, drive } => {
                if let Ok((e, name, position, mut nn)) = pawn_query.get_mut(entity) {
                    let (val, label, need) = match drive {
                        DriveType::Hunger => (&mut nn.hunger, "Hunger", "food"),
                        DriveType::Thirst => (&mut nn.thirst, "Thirst", "water"),
                        DriveType::Fatigue => (&mut nn.fatigue, "Fatigue", "rest"),
                        DriveType::Curiosity => continue,
                    };
                    *val = (*val).min(0.15);
                    quest_board.active_quests.push(Quest {
                        need: need.to_string(),
                        requester: e,
                        chunk: position.chunk,
                        provider: None,
                        status: QuestStatus::Open,
                    });
                    if let Some(ref mut log) = activity {
                        log.push(format!("{} need {} (drive: {})", name, need, label));
                    }
                    if let Some(report) = report.as_deref_mut() {
                        report.bump("dispatcher_drive_threshold_crossed");
                    }
                    if stdout_enabled {
                        eprintln!(
                            "[dispatcher:{}] DriveThresholdCrossed({}) -> pawn marked for re-evaluation, need posted",
                            current_seq, label
                        );
                    }
                }
            }
            CausalEventKind::LeaseReleased { chunk, .. } => {
                if let Some(ref mut log) = activity {
                    log.push(format!("Lease released @ {:?}", chunk));
                }
                if let Some(report) = report.as_deref_mut() {
                    report.bump("dispatcher_lease_released");
                }
                if stdout_enabled {
                    eprintln!("[dispatcher:{}] LeaseReleased on chunk({:?})", current_seq, chunk);
                }
            }
            CausalEventKind::ResourceDepleted { chunk } => {
                if let Some(ref mut log) = activity {
                    log.push(format!("Resource depleted @ {:?}", chunk));
                }
                if let Some(report) = report.as_deref_mut() {
                    report.bump("dispatcher_resource_depleted");
                }
                if stdout_enabled {
                    eprintln!("[dispatcher:{}] ResourceDepleted at chunk({:?})", current_seq, chunk);
                }
            }
        }
    }
}

/// System that runs every frame; drains events at current clock.
pub fn pawn_event_dispatcher_system(
    mut event_queue: ResMut<CausalEventQueue>,
    mut quest_board: ResMut<QuestBoard>,
    global_clock: Res<GlobalTickClock>,
    mut pawn_query: Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
    mut activity: Option<ResMut<ActivityLog>>,
    mut report: Option<ResMut<SimulationReport>>,
    log_settings: Option<Res<SimulationLogSettings>>,
) {
    pawn_event_dispatcher_step(
        global_clock.causal_seq(),
        &mut event_queue,
        &mut quest_board,
        &mut pawn_query,
        activity.as_deref_mut(),
        report.as_deref_mut(),
        stdout_enabled(log_settings.as_deref()),
    );
}

/// Survival priority: hunger/thirst must beat Rest when low. Scale need so Eat/Drink win over Rest early.
const HUNGER_THIRST_SCORE_SCALE: f32 = 2.0;

/// Scorer entities have Actor(pawn); pawn state is on the actor entity.
pub fn hunger_scorer_system(
    mut query: Query<(&Actor, &mut Score), With<HungerScorer>>,
    nn_query: Query<&NeuralNetworkComponent>,
) {
    for (Actor(actor), mut score) in query.iter_mut() {
        if let Ok(nn) = nn_query.get(*actor) {
            let need = (1.0 - nn.hunger).max(0.0);
            let s = (need * HUNGER_THIRST_SCORE_SCALE).min(1.0).max(0.0);
            score.set(s);
        }
    }
}

/// Scorer entities have Actor(pawn); pawn state is on the actor entity.
pub fn thirst_scorer_system(
    mut query: Query<(&Actor, &mut Score), With<ThirstScorer>>,
    nn_query: Query<&NeuralNetworkComponent>,
) {
    for (Actor(actor), mut score) in query.iter_mut() {
        if let Ok(nn) = nn_query.get(*actor) {
            let need = (1.0 - nn.thirst).max(0.0);
            let s = (need * HUNGER_THIRST_SCORE_SCALE).min(1.0).max(0.0);
            score.set(s);
        }
    }
}

/// Scorer entities have Actor(pawn); pawn state is on the actor entity.
pub fn fatigue_scorer_system(
    mut query: Query<(&Actor, &mut Score), With<FatigueScorer>>,
    nn_query: Query<&NeuralNetworkComponent>,
) {
    for (Actor(actor), mut score) in query.iter_mut() {
        if let Ok(nn) = nn_query.get(*actor) {
            let s = (1.0 - nn.fatigue).clamp(0.0, 1.0);
            score.set(s);
        }
    }
}

/// Scores 1.0 when the pawn has a MovementTarget and is not yet on that chunk (so Thinker picks MoveToChunkAction first).
/// Scorer entities have Actor(pawn); Position/MovementTarget are on the actor entity.
pub fn needs_to_move_scorer_system(
    mut query: Query<(&Actor, &mut Score), With<NeedsToMoveScorer>>,
    pawn_query: Query<(&Position, Option<&MovementTarget>)>,
) {
    for (Actor(actor), mut score) in query.iter_mut() {
        if let Ok((position, target)) = pawn_query.get(*actor) {
            let value = target
                .map(|t| if position.chunk != t.0 { 1.0 } else { 0.0 })
                .unwrap_or(0.0);
            score.set(value);
        }
    }
}

/// One Chebyshev step toward MovementTarget per run. No charter lease during transit; lease only at destination for Eat/Drink.
pub fn move_to_chunk_action_system(
    mut commands: Commands,
    mut action_query: Query<(&Actor, &mut ActionState), With<MoveToChunkAction>>,
    mut pawn_query: Query<(&mut Position, &MovementTarget)>,
) {
    for (Actor(actor), mut state) in action_query.iter_mut() {
        match *state {
            ActionState::Requested => {
                if pawn_query.get(*actor).is_ok() {
                    *state = ActionState::Executing;
                } else {
                    *state = ActionState::Failure;
                }
            }
            ActionState::Executing => {
                if let Ok((mut position, target)) = pawn_query.get_mut(*actor) {
                    if position.chunk == target.0 {
                        commands.entity(*actor).remove::<MovementTarget>();
                        *state = ActionState::Success;
                    } else {
                        let dx = (target.0 .0 as i32 - position.chunk.0 as i32).signum();
                        let dy = (target.0 .1 as i32 - position.chunk.1 as i32).signum();
                        let nx = (position.chunk.0 as i32 + dx).clamp(0, CHUNK_EXTENT as i32) as u32;
                        let ny = (position.chunk.1 as i32 + dy).clamp(0, CHUNK_EXTENT as i32) as u32;
                        position.chunk = ChunkId(nx, ny);
                        // Diagnostic: move_to_chunk_action_system actually moved (report is optional)
                        // (report bump would need to be passed in; skip for now)
                    }
                } else {
                    *state = ActionState::Failure;
                }
            }
            ActionState::Cancelled => {
                *state = ActionState::Failure;
            }
            _ => {}
        }
    }
}

pub fn eat_action_system(
    mut commands: Commands,
    mut action_query: Query<(&Actor, &mut ActionState), With<EatAction>>,
    mut nn_query: Query<(&Name, &mut NeuralNetworkComponent, &Position, Option<&ActiveLeaseHandle>)>,
    mut charter: ResMut<SpatialCharter>,
    mut frame_log: ResMut<FrameWriteLog>,
    mut flash: MessageWriter<CharterFlashEvent>,
    global_clock: Res<GlobalTickClock>,
    mut food_query: Query<(Entity, &Position, &mut FoodReservation, Option<&mut ItemHistory>)>,
    mut activity: Option<ResMut<ActivityLog>>,
    mut report: Option<ResMut<SimulationReport>>,
    log_settings: Option<Res<SimulationLogSettings>>,
) {
    let stdout_enabled = stdout_enabled(log_settings.as_deref());
    for (Actor(actor), mut state) in action_query.iter_mut() {
        if let Ok((name, mut nn, pawn_pos, lease_handle)) = nn_query.get_mut(*actor) {
            match *state {
                ActionState::Requested => {
                    // Find any food with portions (goal-directed movement: we may be en route).
                    let food_with_portions =
                        food_query.iter().find(|(_, _, res, _)| res.portions > 0);
                    if let Some((_, food_pos, _, _)) = food_with_portions {
                        if pawn_pos.chunk != food_pos.chunk {
                            // Not on the food chunk: set target so Thinker picks MoveToChunkAction; fail this action so we re-evaluate.
                            commands.entity(*actor).insert(MovementTarget(food_pos.chunk));
                            *state = ActionState::Failure;
                            continue;
                        }
                        // On the food chunk: request lease and execute.
                        let request = SpatialLease {
                             primary: food_pos.chunk,
                             fringe: vec![],
                             intent: LeaseIntent {
                                 reads: vec![],
                                 writes: vec![TypeId::of::<FoodReservation>()],
                             },
                             granted_at_causal_seq: global_clock.causal_seq(),
                         };

                         match charter.request_lease(request, global_clock.causal_seq()) {
                             Ok(handle) => {
                                 flash.write(CharterFlashEvent { chunk: food_pos.chunk, granted: true });
                                 if let Some(ref mut log) = activity {
                                     log.push(format!("{} eating @ {:?}", name, food_pos.chunk));
                                 }
                                 if let Some(ref mut report) = report {
                                     report.bump("eat_lease_granted");
                                 }
                                 if stdout_enabled {
                                     println!("[causal:{}] {} requested lease on chunk({:?}) for Write(FoodReservation) - GRANTED", global_clock.causal_seq(), name, food_pos.chunk);
                                 }
                                 commands.entity(*actor).insert(ActiveLeaseHandle(handle));
                                 *state = ActionState::Executing;
                             }
                             Err(CharterDenial::ChunkConflict { contested, retry_after_causal_seq, .. }) => {
                                 for c in &contested {
                                     flash.write(CharterFlashEvent { chunk: *c, granted: false });
                                 }
                                 if let Some(ref mut report) = report {
                                     report.bump("eat_lease_denied");
                                 }
                                 if stdout_enabled {
                                     println!("[causal:{}] {} requested lease on chunk({:?}) for Write(FoodReservation) - DENIED (ChunkConflict, retry after causal:{})", global_clock.causal_seq(), name, food_pos.chunk, retry_after_causal_seq);
                                 }
                             }
                             Err(_) => {}
                         }
                    } else {
                         // No food found in world
                         *state = ActionState::Failure;
                    }
                }
                ActionState::Executing => {
                    if let Some(handle) = lease_handle {
                        let local_food = food_query.iter_mut().find(|(_, f_pos, _, _)| f_pos.chunk == pawn_pos.chunk);

                        if let Some((food_entity, food_pos, mut food_res, item_hist)) = local_food {
                            if food_res.portions > 0 {
                                if let Some(ref mut log) = activity {
                                    log.push(format!("{} ate", name));
                                }
                                if let Some(ref mut report) = report {
                                    report.bump("eat_action_completed");
                                }
                                if stdout_enabled {
                                    println!("[causal:{}] {} ate a portion of the food reservation!", global_clock.causal_seq(), name);
                                }
                                if let Some(mut hist) = item_hist {
                                    hist.record(*actor, "ate", global_clock.causal_seq());
                                }
                                food_res.portions -= 1;
                                nn.hunger += 0.5;
                                nn.hunger = nn.hunger.min(1.0);

                                if food_res.portions == 0 {
                                    if let Some(ref mut log) = activity {
                                        log.push("Food depleted".into());
                                    }
                                    if let Some(ref mut report) = report {
                                        report.bump("food_depleted");
                                    }
                                    if stdout_enabled {
                                        println!("[causal:{}] Food depleted!", global_clock.causal_seq());
                                    }
                                    commands.entity(food_entity).despawn();
                                }
                            }
                            let snapshot = charter.get_lease(handle.0).cloned();
                            frame_log.log(*actor, food_pos.chunk, TypeId::of::<FoodReservation>(), snapshot);
                            if stdout_enabled {
                                println!("[causal:{}] {} released lease on chunk({:?})", global_clock.causal_seq(), name, food_pos.chunk);
                            }
                            charter.release_lease(handle.0);
                            commands.entity(*actor).remove::<ActiveLeaseHandle>();
                            *state = ActionState::Success;
                        } else {
                            // Food disappeared or lease was somehow invalid
                            charter.release_lease(handle.0);
                            commands.entity(*actor).remove::<ActiveLeaseHandle>();
                            *state = ActionState::Failure;
                        }
                    } else {
                        // Shouldn't happen unless state machine corrupted
                        *state = ActionState::Failure;
                    }
                }
                ActionState::Cancelled => {
                    if let Some(handle) = lease_handle {
                        charter.release_lease(handle.0);
                        commands.entity(*actor).remove::<ActiveLeaseHandle>();
                    }
                    *state = ActionState::Failure;
                }
                _ => {}
            }
        }
    }
}

pub fn drink_action_system(
    mut commands: Commands,
    mut action_query: Query<(&Actor, &mut ActionState), With<DrinkAction>>,
    mut nn_query: Query<(&Name, &mut NeuralNetworkComponent, &Position, Option<&ActiveLeaseHandle>)>,
    mut charter: ResMut<SpatialCharter>,
    mut frame_log: ResMut<FrameWriteLog>,
    mut flash: MessageWriter<CharterFlashEvent>,
    global_clock: Res<GlobalTickClock>,
    mut water_query: Query<(Entity, &Position, &mut WaterSource, Option<&mut ItemHistory>)>,
    mut activity: Option<ResMut<ActivityLog>>,
    mut report: Option<ResMut<SimulationReport>>,
    log_settings: Option<Res<SimulationLogSettings>>,
) {
    let stdout_enabled = stdout_enabled(log_settings.as_deref());
    for (Actor(actor), mut state) in action_query.iter_mut() {
        if let Ok((name, mut nn, pawn_pos, lease_handle)) = nn_query.get_mut(*actor) {
            match *state {
                ActionState::Requested => {
                    let water_with_portions =
                        water_query.iter().find(|(_, _, src, _)| src.portions > 0);
                    if let Some((_, water_pos, _, _)) = water_with_portions {
                        if pawn_pos.chunk != water_pos.chunk {
                            if let Some(ref mut report) = report {
                                report.bump("drink_request_not_on_chunk");
                            }
                            commands.entity(*actor).insert(MovementTarget(water_pos.chunk));
                            *state = ActionState::Failure;
                            continue;
                        }
                        if let Some(ref mut report) = report {
                            report.bump("drink_request_on_chunk");
                        }
                        let request = SpatialLease {
                            primary: water_pos.chunk,
                            fringe: vec![],
                            intent: LeaseIntent {
                                reads: vec![],
                                writes: vec![TypeId::of::<WaterSource>()],
                            },
                            granted_at_causal_seq: global_clock.causal_seq(),
                        };
                        match charter.request_lease(request, global_clock.causal_seq()) {
                            Ok(handle) => {
                                flash.write(CharterFlashEvent { chunk: water_pos.chunk, granted: true });
                                if let Some(ref mut log) = activity {
                                    log.push(format!("{} drinking @ {:?}", name, water_pos.chunk));
                                }
                                if let Some(ref mut report) = report {
                                    report.bump("drink_lease_granted");
                                }
                                if stdout_enabled {
                                    println!("[causal:{}] {} requested lease on chunk({:?}) for Write(WaterSource) - GRANTED", global_clock.causal_seq(), name, water_pos.chunk);
                                }
                                commands.entity(*actor).insert(ActiveLeaseHandle(handle));
                                *state = ActionState::Executing;
                            }
                            Err(CharterDenial::ChunkConflict { contested, retry_after_causal_seq, .. }) => {
                                for c in &contested {
                                    flash.write(CharterFlashEvent { chunk: *c, granted: false });
                                }
                                if let Some(ref mut report) = report {
                                    report.bump("drink_lease_denied");
                                }
                                if stdout_enabled {
                                    println!("[causal:{}] {} DrinkAction lease DENIED (retry after {})", global_clock.causal_seq(), name, retry_after_causal_seq);
                                }
                            }
                            Err(_) => {}
                        }
                    } else {
                        // No water with portions in world
                        *state = ActionState::Failure;
                    }
                }
                ActionState::Executing => {
                    if let Some(handle) = lease_handle {
                        let local_water =
                            water_query.iter_mut().find(|(_, w_pos, _, _)| w_pos.chunk == pawn_pos.chunk);
                        if let Some((water_entity, water_pos, mut water_src, item_hist)) = local_water {
                            if water_src.portions > 0 {
                                if let Some(ref mut log) = activity {
                                    log.push(format!("{} drank", name));
                                }
                                if let Some(ref mut report) = report {
                                    report.bump("drink_action_completed");
                                }
                                if stdout_enabled {
                                    println!("[causal:{}] {} drank a portion!", global_clock.causal_seq(), name);
                                }
                                if let Some(mut hist) = item_hist {
                                    hist.record(*actor, "drank", global_clock.causal_seq());
                                }
                                water_src.portions -= 1;
                                nn.thirst += 0.5;
                                nn.thirst = nn.thirst.min(1.0);
                                if water_src.portions == 0 {
                                    if let Some(ref mut report) = report {
                                        report.bump("water_depleted");
                                    }
                                    commands.entity(water_entity).despawn();
                                }
                            }
                            let snapshot = charter.get_lease(handle.0).cloned();
                            frame_log.log(*actor, water_pos.chunk, TypeId::of::<WaterSource>(), snapshot);
                            charter.release_lease(handle.0);
                            commands.entity(*actor).remove::<ActiveLeaseHandle>();
                            *state = ActionState::Success;
                        } else {
                            charter.release_lease(handle.0);
                            commands.entity(*actor).remove::<ActiveLeaseHandle>();
                            *state = ActionState::Failure;
                        }
                    } else {
                        *state = ActionState::Failure;
                    }
                }
                ActionState::Cancelled => {
                    if let Some(handle) = lease_handle {
                        charter.release_lease(handle.0);
                        commands.entity(*actor).remove::<ActiveLeaseHandle>();
                    }
                    *state = ActionState::Failure;
                }
                _ => {}
            }
        }
    }
}

const REST_RECOVERY_PER_STEP: f32 = 0.12;
const REST_FATIGUE_TARGET: f32 = 0.95;

pub fn rest_action_system(
    mut commands: Commands,
    mut action_query: Query<(&Actor, &mut ActionState), With<RestAction>>,
    mut nn_query: Query<(&Name, &mut NeuralNetworkComponent, &Position, Option<&ActiveLeaseHandle>)>,
    mut charter: ResMut<SpatialCharter>,
    mut flash: MessageWriter<CharterFlashEvent>,
    global_clock: Res<GlobalTickClock>,
    rest_query: Query<(&Position, &RestSpot)>,
    mut activity: Option<ResMut<ActivityLog>>,
    mut report: Option<ResMut<SimulationReport>>,
    log_settings: Option<Res<SimulationLogSettings>>,
) {
    let stdout_enabled = stdout_enabled(log_settings.as_deref());
    for (Actor(actor), mut state) in action_query.iter_mut() {
        if let Ok((name, mut nn, pawn_pos, lease_handle)) = nn_query.get_mut(*actor) {
            match *state {
                ActionState::Requested => {
                    let any_rest = rest_query.iter().next();
                    if let Some((rest_pos, _)) = any_rest {
                        if pawn_pos.chunk != rest_pos.chunk {
                            commands.entity(*actor).insert(MovementTarget(rest_pos.chunk));
                            *state = ActionState::Failure;
                            continue;
                        }
                        let request = SpatialLease {
                            primary: rest_pos.chunk,
                            fringe: vec![],
                            intent: LeaseIntent {
                                reads: vec![TypeId::of::<RestSpot>()],
                                writes: vec![],
                            },
                            granted_at_causal_seq: global_clock.causal_seq(),
                        };
                        match charter.request_lease(request, global_clock.causal_seq()) {
                            Ok(handle) => {
                                flash.write(CharterFlashEvent { chunk: rest_pos.chunk, granted: true });
                                if let Some(ref mut log) = activity {
                                    log.push(format!("{} resting @ {:?}", name, rest_pos.chunk));
                                }
                                if let Some(ref mut report) = report {
                                    report.bump("rest_lease_granted");
                                }
                                if stdout_enabled {
                                    println!("[causal:{}] {} requested read lease on chunk({:?}) for Rest - GRANTED", global_clock.causal_seq(), name, rest_pos.chunk);
                                }
                                commands.entity(*actor).insert(ActiveLeaseHandle(handle));
                                *state = ActionState::Executing;
                            }
                            Err(CharterDenial::ChunkConflict { contested, .. }) => {
                                for c in &contested {
                                    flash.write(CharterFlashEvent { chunk: *c, granted: false });
                                }
                                if let Some(ref mut report) = report {
                                    report.bump("rest_lease_denied");
                                }
                            }
                            Err(_) => {}
                        }
                    } else {
                        *state = ActionState::Failure;
                    }
                }
                ActionState::Executing => {
                    if lease_handle.is_some() {
                        nn.fatigue += REST_RECOVERY_PER_STEP;
                        nn.fatigue = nn.fatigue.min(1.0);
                        if nn.fatigue >= REST_FATIGUE_TARGET {
                            if let Some(ref mut log) = activity {
                                log.push(format!("{} rested", name));
                            }
                            if let Some(ref mut report) = report {
                                report.bump("rest_action_completed");
                            }
                            if let Some(handle) = lease_handle {
                                if stdout_enabled {
                                    println!("[causal:{}] {} rested enough, releasing lease", global_clock.causal_seq(), name);
                                }
                                charter.release_lease(handle.0);
                                commands.entity(*actor).remove::<ActiveLeaseHandle>();
                            }
                            *state = ActionState::Success;
                        }
                    } else {
                        *state = ActionState::Failure;
                    }
                }
                ActionState::Cancelled => {
                    if let Some(handle) = lease_handle {
                        charter.release_lease(handle.0);
                        commands.entity(*actor).remove::<ActiveLeaseHandle>();
                    }
                    *state = ActionState::Failure;
                }
                _ => {}
            }
        }
    }
}

pub fn build_pawn_brain() -> ThinkerBuilder {
    Thinker::build()
        .label("Pawn Brain")
        .picker(FirstToScore { threshold: 0.8 })
        .when(NeedsToMoveScorer, MoveToChunkAction)
        .when(HungerScorer, EatAction)
        .when(ThirstScorer, DrinkAction)
        .when(FatigueScorer, RestAction)
}
