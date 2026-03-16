use bevy::prelude::*;
use bevy::ecs::message::MessageWriter;
use simrard_lib_mirror::{rank_provider_candidates_for_need, ProviderCandidateInput};
use simrard_lib_utility_ai::prelude::*;
use simrard_lib_causal::{CausalEventKind, CausalEventQueue, DriveType};
use simrard_lib_charter::{
    CharterDenial, CharterFlashEvent, ChunkId, FrameWriteLog, LeaseIntent, SpatialCharter,
    SpatialLease,
};
use simrard_lib_pawn::{
    ActiveLeaseHandle, Capabilities, FoodReservation, ItemHistory, MovementTarget,
    KnownRecipes, MortalityCause, NeuralNetworkComponent, PawnDeathRecord, Position, Quest,
    QuestBoard, QuestStatus, RestSpot, SimulationLogSettings, SimulationReport, WaterSource,
    WORLD_CHUNK_EXTENT,
};
use simrard_lib_time::{CausalClock, GlobalTickClock};
use std::any::TypeId;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

fn stdout_enabled(settings: Option<&SimulationLogSettings>) -> bool {
    settings.map(|settings| settings.stdout_enabled).unwrap_or(true)
}

/// Optional UI feed: when present, action systems and dispatcher push short activity strings.
/// Bin inits this and displays the last N lines.
#[derive(Resource, Default)]
pub struct ActivityLog(pub VecDeque<String>);

const ACTIVITY_LOG_MAX: usize = 32;
const QUEST_ACCEPTANCE_MIN_DRIVE: f32 = 0.3;

#[derive(Resource, Default)]
pub struct DispatcherEvaluationState {
    pub dirty_all: bool,
    pub dirty_entities: HashSet<Entity>,
    pub last_evaluation_tick: HashMap<Entity, u64>,
    pub last_region_signature: HashMap<Entity, u64>,
}

impl DispatcherEvaluationState {
    pub fn mark_dirty(&mut self, entity: Entity) {
        self.dirty_entities.insert(entity);
    }

    pub fn mark_all_dirty(&mut self) {
        self.dirty_all = true;
    }
}

#[derive(Default)]
struct QuestAcceptanceTiming {
    utility_score_calc_us: u64,
    best_action_selection_us: u64,
    action_execution_us: u64,
}

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

/// Chunk grid extent (0..=WORLD_CHUNK_EXTENT). Matches bin visualizer; movement is clamped to this range.
const CHUNK_EXTENT: u32 = WORLD_CHUNK_EXTENT;

pub struct PawnAIPlugin;

impl Plugin for PawnAIPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DispatcherEvaluationState>();
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
    query: Query<(Entity, &Name, &Position, &NeuralNetworkComponent)>,
    mut activity: Option<ResMut<ActivityLog>>,
    mut quest_board: Option<ResMut<QuestBoard>>,
    global_clock: Option<Res<GlobalTickClock>>,
    mut report: Option<ResMut<SimulationReport>>,
) {
    let to_despawn: Vec<(Entity, String, ChunkId, NeuralNetworkComponent)> = query
        .iter()
        .filter(|(_, _, _, nn)| nn.hunger <= 0.0 || nn.thirst <= 0.0)
        .map(|(e, name, pos, nn)| (e, format!("{}", name), pos.chunk, nn.clone()))
        .collect();
    let dead: std::collections::HashSet<_> = to_despawn.iter().map(|(e, _, _, _)| *e).collect();
    if let Some(ref mut board) = quest_board {
        board.active_quests.retain(|q| !dead.contains(&q.requester));
    }
    for (entity, name, chunk, nn) in to_despawn {
        let (cause, primary_drive) = classify_mortality(&nn);
        if let Some(ref mut log) = activity {
            log.push(format!("{} died (hunger/thirst zero)", name));
        }
        if let Some(ref mut report) = report {
            report.bump("pawn_deaths");
            match cause {
                MortalityCause::Hunger => report.bump("deaths_hunger"),
                MortalityCause::Thirst => report.bump("deaths_thirst"),
                MortalityCause::Other => report.bump("deaths_other"),
            }
            let tick = global_clock
                .as_deref()
                .map(CausalClock::causal_seq)
                .unwrap_or_default();
            report.record_death(PawnDeathRecord {
                tick,
                pawn_name: name.clone(),
                cause,
                primary_drive,
                hunger: nn.hunger,
                thirst: nn.thirst,
                fatigue: nn.fatigue,
                curiosity: nn.curiosity,
                social: nn.social,
                fear: nn.fear,
                industriousness: nn.industriousness,
                comfort: nn.comfort,
                chunk,
            });
            report.note(format!(
                "tick {}: {} died cause={:?} primary={} @ {:?}",
                tick, name, cause, primary_drive, chunk
            ));
        }
        commands.entity(entity).despawn();
    }
}

fn classify_mortality(nn: &NeuralNetworkComponent) -> (MortalityCause, &'static str) {
    let drives = [
        ("hunger", nn.hunger),
        ("thirst", nn.thirst),
        ("fatigue", nn.fatigue),
        ("curiosity", nn.curiosity),
        ("social", nn.social),
        ("fear", nn.fear),
        ("industriousness", nn.industriousness),
        ("comfort", nn.comfort),
    ];
    let primary_drive = drives
        .iter()
        .min_by(|a, b| a.1.partial_cmp(&b.1).expect("drive values must be finite"))
        .map(|(name, _)| *name)
        .expect("drive vector must not be empty");

    let cause = if nn.hunger <= 0.0 && nn.thirst <= 0.0 {
        if nn.hunger <= nn.thirst {
            MortalityCause::Hunger
        } else {
            MortalityCause::Thirst
        }
    } else if nn.hunger <= 0.0 {
        MortalityCause::Hunger
    } else if nn.thirst <= 0.0 {
        MortalityCause::Thirst
    } else {
        MortalityCause::Other
    };

    (cause, primary_drive)
}

/// One dispatcher step: drain events ready at `current_seq` and apply them.
/// Used by both the standalone system and the sim tick driver.
pub fn pawn_event_dispatcher_step(
    current_seq: u64,
    event_queue: &mut CausalEventQueue,
    quest_board: &mut QuestBoard,
    pawn_query: &mut Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
    capabilities_query: &Query<&Capabilities>,
    known_recipes_query: &mut Query<&mut KnownRecipes>,
    region_signatures: &HashMap<Entity, u64>,
    mut activity: Option<&mut ActivityLog>,
    mut report: Option<&mut SimulationReport>,
    evaluation_state: &mut DispatcherEvaluationState,
    stdout_enabled: bool,
) {
    for (entity, signature) in region_signatures {
        let prev = evaluation_state.last_region_signature.insert(*entity, *signature);
        match prev {
            Some(old) if old == *signature => {}
            _ => evaluation_state.mark_dirty(*entity),
        }
    }

    let collect_started = Instant::now();
    let ready_events = event_queue.drain_ready(current_seq);
    let collect_elapsed = collect_started.elapsed();

    let mut drive_updates_us: u64 = 0;
    let mut lease_request_path_us: u64 = 0;
    let mut other_event_us: u64 = 0;

    for event in ready_events {
        let event_started = Instant::now();
        match event.kind {
            CausalEventKind::DriveThresholdCrossed { entity, drive } => {
                evaluation_state.mark_all_dirty();
                evaluation_state.mark_dirty(entity);
                if let Ok((e, name, position, mut nn)) = pawn_query.get_mut(entity) {
                    let (val, label, need) = match drive {
                        DriveType::Hunger => (&mut nn.hunger, "Hunger", "food"),
                        DriveType::Thirst => (&mut nn.thirst, "Thirst", "water"),
                        DriveType::Fatigue => (&mut nn.fatigue, "Fatigue", "rest"),
                        DriveType::Curiosity => continue,
                    };
                    // #scheduler-debt: clamp the triggered drive down to force near-term
                    // re-scoring in the current utility-ai scheduler model.
                    // TODO(2026-03-15): replace this mutation with event-reactive
                    // thinker wakeups once scheduler support exists.
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
                drive_updates_us += event_started.elapsed().as_micros() as u64;
            }
            CausalEventKind::LeaseDenied { entity, chunk, .. } => {
                evaluation_state.mark_dirty(entity);
                if let Some(ref mut log) = activity {
                    log.push(format!("Lease denied @ {:?} for {:?}", chunk, entity));
                }
                if let Some(report) = report.as_deref_mut() {
                    report.bump("dispatcher_lease_denied_event");
                }
                lease_request_path_us += event_started.elapsed().as_micros() as u64;
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
                lease_request_path_us += event_started.elapsed().as_micros() as u64;
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
                other_event_us += event_started.elapsed().as_micros() as u64;
            }
            CausalEventKind::DiscoveryPropagated { recipe, from, to } => {
                if let Ok(mut known) = known_recipes_query.get_mut(to) {
                    if known.recipes.insert(recipe.clone()) {
                        if let Some(ref mut log) = activity {
                            log.push(format!("{:?} learned {} from {:?}", to, recipe, from));
                        }
                        if let Some(report) = report.as_deref_mut() {
                            report.bump("dispatcher_discovery_propagated");
                        }
                        if stdout_enabled {
                            eprintln!(
                                "[dispatcher:{}] DiscoveryPropagated recipe={} from={:?} to={:?}",
                                current_seq, recipe, from, to
                            );
                        }
                    }
                }
                other_event_us += event_started.elapsed().as_micros() as u64;
            }
        }
    }

    let action_started = Instant::now();
    let timing = quest_acceptance_step(
        current_seq,
        quest_board,
        pawn_query,
        capabilities_query,
        activity,
        report.as_deref_mut(),
        evaluation_state,
        stdout_enabled,
    );
    let action_elapsed = action_started.elapsed();

    if let Some(report) = report.as_deref_mut() {
        report.bump("dispatcher_phase_samples");
        report.add_counter(
            "dispatcher_event_collection_us",
            collect_elapsed.as_micros() as u64,
        );
        report.add_counter("dispatcher_drive_updates_us", drive_updates_us);
        report.add_counter("dispatcher_lease_requests_us", lease_request_path_us);
        report.add_counter("dispatcher_action_resolution_us", action_elapsed.as_micros() as u64);
        report.add_counter("dispatcher_score_calc_us", timing.utility_score_calc_us);
        report.add_counter("dispatcher_best_action_select_us", timing.best_action_selection_us);
        report.add_counter("dispatcher_action_execute_us", timing.action_execution_us);
        report.add_counter("dispatcher_other_event_us", other_event_us);
    }
}

fn need_capability_and_drive(need: &str, nn: &NeuralNetworkComponent) -> Option<(&'static str, f32)> {
    match need {
        "food" => Some(("Eat", (1.0 - nn.hunger).clamp(0.0, 1.0))),
        "water" => Some(("Drink", (1.0 - nn.thirst).clamp(0.0, 1.0))),
        "rest" => Some(("Rest", (1.0 - nn.fatigue).clamp(0.0, 1.0))),
        _ => None,
    }
}

fn chebyshev_distance(a: ChunkId, b: ChunkId) -> u32 {
    a.0.abs_diff(b.0).max(a.1.abs_diff(b.1))
}

fn quest_acceptance_step(
    current_seq: u64,
    quest_board: &mut QuestBoard,
    pawn_query: &mut Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
    capabilities_query: &Query<&Capabilities>,
    mut activity: Option<&mut ActivityLog>,
    mut report: Option<&mut SimulationReport>,
    evaluation_state: &mut DispatcherEvaluationState,
    stdout_enabled: bool,
) -> QuestAcceptanceTiming {
    let mut timing = QuestAcceptanceTiming::default();

    // Keep completed quests visible until next tick, then clear them.
    let completed_before = quest_board
        .active_quests
        .iter()
        .filter(|q| matches!(q.status, QuestStatus::Completed))
        .count();
    if completed_before > 0 {
        quest_board
            .active_quests
            .retain(|q| !matches!(q.status, QuestStatus::Completed));
    }

    let has_open_quests = quest_board
        .active_quests
        .iter()
        .any(|q| matches!(q.status, QuestStatus::Open));
    if !has_open_quests {
        return timing;
    }

    let should_evaluate = evaluation_state.dirty_all || !evaluation_state.dirty_entities.is_empty();
    if !should_evaluate {
        if let Some(report) = report.as_deref_mut() {
            report.bump("dispatcher_eval_skipped_clean");
        }
        return timing;
    }

    let mut pawns: Vec<(Entity, String, ChunkId, Option<Capabilities>, f32, f32, f32)> = Vec::new();
    let mut pawn_name_by_entity: HashMap<Entity, String> = HashMap::new();
    let dirty_all = evaluation_state.dirty_all;
    for (entity, name, position, nn) in pawn_query.iter_mut() {
        if !dirty_all && !evaluation_state.dirty_entities.contains(&entity) {
            continue;
        }
        match evaluation_state.last_evaluation_tick.get(&entity) {
            Some(last) if *last == current_seq => continue,
            _ => {}
        }
        let name_string = name.to_string();
        pawn_name_by_entity.insert(entity, name_string.clone());
        let capabilities = capabilities_query.get(entity).ok().cloned();
        pawns.push((
            entity,
            name_string,
            position.chunk,
            capabilities,
            (1.0 - nn.hunger).clamp(0.0, 1.0),
            (1.0 - nn.thirst).clamp(0.0, 1.0),
            (1.0 - nn.fatigue).clamp(0.0, 1.0),
        ));
        evaluation_state.last_evaluation_tick.insert(entity, current_seq);
    }

    if pawns.is_empty() {
        evaluation_state.dirty_entities.clear();
        evaluation_state.dirty_all = false;
        if let Some(report) = report.as_deref_mut() {
            report.bump("dispatcher_eval_skipped_no_dirty_candidates");
        }
        return timing;
    }

    let mut providers_in_progress: HashSet<Entity> = quest_board
        .active_quests
        .iter()
        .filter_map(|q| match q.status {
            QuestStatus::InProgress { provider } => Some(provider),
            _ => None,
        })
        .collect();

    for quest in quest_board.active_quests.iter_mut() {
        if !matches!(quest.status, QuestStatus::Open) {
            continue;
        }

        let Some((required_capability, _)) = need_capability_and_drive(&quest.need, &NeuralNetworkComponent::default()) else {
            continue;
        };

        let mut eligible_candidates: Vec<(u32, Entity, String, f32)> = Vec::new();
        let mut rank_inputs: Vec<ProviderCandidateInput> = Vec::new();
        let score_started = Instant::now();
        for (entity, name, chunk, capabilities, food_drive, water_drive, rest_drive) in &pawns {
            if *entity == quest.requester || providers_in_progress.contains(entity) {
                continue;
            }
            let Some(caps) = capabilities.as_ref() else {
                continue;
            };
            if !caps.has(required_capability) {
                continue;
            }

            let drive = match quest.need.as_str() {
                "food" => *food_drive,
                "water" => *water_drive,
                "rest" => *rest_drive,
                _ => 0.0,
            };
            if drive < QUEST_ACCEPTANCE_MIN_DRIVE {
                continue;
            }

            let dist = chebyshev_distance(*chunk, quest.chunk);
            let candidate_id = eligible_candidates.len() as u32;
            eligible_candidates.push((candidate_id, *entity, name.clone(), drive));
            rank_inputs.push(ProviderCandidateInput {
                candidate_id,
                drive,
                proximity: 1.0 / (dist + 1) as f32,
                distance: dist,
                can_eat: caps.has("Eat"),
                can_drink: caps.has("Drink"),
                can_rest: caps.has("Rest"),
            });
        }
        timing.utility_score_calc_us += score_started.elapsed().as_micros() as u64;

        let select_started = Instant::now();

        let Some(selected_candidate_id) = rank_provider_candidates_for_need(
            &quest.need,
            QUEST_ACCEPTANCE_MIN_DRIVE,
            &rank_inputs,
        )
        .expect("Quest acceptance provider ranking via DuckDB failed") else {
            continue;
        };

        let Some((_, provider, provider_name, drive)) = eligible_candidates
            .iter()
            .find(|(candidate_id, _, _, _)| *candidate_id == selected_candidate_id)
            .cloned()
        else {
            panic!(
                "Quest acceptance ranked candidate id {} missing from eligible set",
                selected_candidate_id
            );
        };
        timing.best_action_selection_us += select_started.elapsed().as_micros() as u64;

        let execute_started = Instant::now();

            quest.provider = Some(provider);
            quest.status = QuestStatus::InProgress { provider };
            providers_in_progress.insert(provider);
            if let Some(ref mut log) = activity {
                let requester = pawn_name_by_entity
                    .get(&quest.requester)
                    .cloned()
                    .unwrap_or_else(|| format!("{:?}", quest.requester));
                log.push(format!(
                    "{} accepted {} quest for {}",
                    provider_name, quest.need, requester
                ));
            }
            if let Some(report) = report.as_deref_mut() {
                report.bump("quest_acceptances");
            }
            if stdout_enabled {
                eprintln!(
                    "[dispatcher:{}] Quest accepted: need={} provider={:?} drive={:.2}",
                    current_seq, quest.need, provider, drive
                );
            }
        timing.action_execution_us += execute_started.elapsed().as_micros() as u64;
    }

    evaluation_state.dirty_entities.clear();
    evaluation_state.dirty_all = false;
    timing
}

pub fn complete_quest_for_action(
    quest_board: &mut QuestBoard,
    provider: Entity,
    need: &str,
    chunk: ChunkId,
) -> bool {
    for quest in quest_board.active_quests.iter_mut() {
        if let QuestStatus::InProgress {
            provider: quest_provider,
        } = quest.status
        {
            if quest_provider == provider && quest.need == need && quest.chunk == chunk {
                quest.status = QuestStatus::Completed;
                return true;
            }
        }
    }
    false
}

/// System that runs every frame; drains events at current clock.
pub fn pawn_event_dispatcher_system(
    mut event_queue: ResMut<CausalEventQueue>,
    mut quest_board: ResMut<QuestBoard>,
    global_clock: Res<GlobalTickClock>,
    mut pawn_query: Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
    capabilities_query: Query<&Capabilities>,
    mut known_recipes_query: Query<&mut KnownRecipes>,
    mut activity: Option<ResMut<ActivityLog>>,
    mut report: Option<ResMut<SimulationReport>>,
    mut evaluation_state: ResMut<DispatcherEvaluationState>,
    log_settings: Option<Res<SimulationLogSettings>>,
) {
    let region_signatures: HashMap<Entity, u64> = HashMap::new();
    pawn_event_dispatcher_step(
        global_clock.causal_seq(),
        &mut event_queue,
        &mut quest_board,
        &mut pawn_query,
        &capabilities_query,
        &mut known_recipes_query,
        &region_signatures,
        activity.as_deref_mut(),
        report.as_deref_mut(),
        &mut evaluation_state,
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
    mut evaluation_state: ResMut<DispatcherEvaluationState>,
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
                        // Charter-region change trigger: crossing chunk boundary invalidates prior local evaluation.
                        evaluation_state.mark_dirty(*actor);
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
    mut event_queue: ResMut<CausalEventQueue>,
    mut food_query: Query<(Entity, &Position, &mut FoodReservation, Option<&mut ItemHistory>)>,
    mut quest_board: ResMut<QuestBoard>,
    mut evaluation_state: ResMut<DispatcherEvaluationState>,
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

                         let lease_started = Instant::now();
                         match charter.request_lease(request, global_clock.causal_seq()) {
                             Ok(handle) => {
                                 if let Some(ref mut report) = report {
                                     report.add_counter("dispatcher_lease_requests_us", lease_started.elapsed().as_micros() as u64);
                                 }
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
                                 if let Some(ref mut report) = report {
                                     report.add_counter("dispatcher_lease_requests_us", lease_started.elapsed().as_micros() as u64);
                                 }
                                 for c in &contested {
                                     flash.write(CharterFlashEvent { chunk: *c, granted: false });
                                 }
                                 if let Some(ref mut report) = report {
                                     report.bump("eat_lease_denied");
                                 }
                                 evaluation_state.mark_dirty(*actor);
                                 event_queue.push_at(
                                     CausalEventKind::LeaseDenied {
                                         entity: *actor,
                                         chunk: food_pos.chunk,
                                         component: TypeId::of::<FoodReservation>(),
                                     },
                                     food_pos.chunk,
                                     global_clock.causal_seq(),
                                 );
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

                                if complete_quest_for_action(
                                    &mut quest_board,
                                    *actor,
                                    "food",
                                    pawn_pos.chunk,
                                ) {
                                    if let Some(ref mut log) = activity {
                                        log.push(format!("{} completed food quest", name));
                                    }
                                    if let Some(ref mut report) = report {
                                        report.bump("quest_completed_food");
                                    }
                                }

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
    mut event_queue: ResMut<CausalEventQueue>,
    mut water_query: Query<(Entity, &Position, &mut WaterSource, Option<&mut ItemHistory>)>,
    mut quest_board: ResMut<QuestBoard>,
    mut evaluation_state: ResMut<DispatcherEvaluationState>,
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
                        let lease_started = Instant::now();
                        match charter.request_lease(request, global_clock.causal_seq()) {
                            Ok(handle) => {
                                if let Some(ref mut report) = report {
                                    report.add_counter("dispatcher_lease_requests_us", lease_started.elapsed().as_micros() as u64);
                                }
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
                                if let Some(ref mut report) = report {
                                    report.add_counter("dispatcher_lease_requests_us", lease_started.elapsed().as_micros() as u64);
                                }
                                for c in &contested {
                                    flash.write(CharterFlashEvent { chunk: *c, granted: false });
                                }
                                if let Some(ref mut report) = report {
                                    report.bump("drink_lease_denied");
                                }
                                evaluation_state.mark_dirty(*actor);
                                event_queue.push_at(
                                    CausalEventKind::LeaseDenied {
                                        entity: *actor,
                                        chunk: water_pos.chunk,
                                        component: TypeId::of::<WaterSource>(),
                                    },
                                    water_pos.chunk,
                                    global_clock.causal_seq(),
                                );
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

                                if complete_quest_for_action(
                                    &mut quest_board,
                                    *actor,
                                    "water",
                                    pawn_pos.chunk,
                                ) {
                                    if let Some(ref mut log) = activity {
                                        log.push(format!("{} completed water quest", name));
                                    }
                                    if let Some(ref mut report) = report {
                                        report.bump("quest_completed_water");
                                    }
                                }

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
    mut event_queue: ResMut<CausalEventQueue>,
    rest_query: Query<(&Position, &RestSpot)>,
    mut quest_board: ResMut<QuestBoard>,
    mut evaluation_state: ResMut<DispatcherEvaluationState>,
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
                        let lease_started = Instant::now();
                        match charter.request_lease(request, global_clock.causal_seq()) {
                            Ok(handle) => {
                                if let Some(ref mut report) = report {
                                    report.add_counter("dispatcher_lease_requests_us", lease_started.elapsed().as_micros() as u64);
                                }
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
                                if let Some(ref mut report) = report {
                                    report.add_counter("dispatcher_lease_requests_us", lease_started.elapsed().as_micros() as u64);
                                }
                                for c in &contested {
                                    flash.write(CharterFlashEvent { chunk: *c, granted: false });
                                }
                                if let Some(ref mut report) = report {
                                    report.bump("rest_lease_denied");
                                }
                                evaluation_state.mark_dirty(*actor);
                                event_queue.push_at(
                                    CausalEventKind::LeaseDenied {
                                        entity: *actor,
                                        chunk: rest_pos.chunk,
                                        component: TypeId::of::<RestSpot>(),
                                    },
                                    rest_pos.chunk,
                                    global_clock.causal_seq(),
                                );
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
                            if complete_quest_for_action(
                                &mut quest_board,
                                *actor,
                                "rest",
                                pawn_pos.chunk,
                            ) {
                                if let Some(ref mut log) = activity {
                                    log.push(format!("{} completed rest quest", name));
                                }
                                if let Some(ref mut report) = report {
                                    report.bump("quest_completed_rest");
                                }
                            }
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
        .repeat_action_cooldown_ticks(3)
        .picker(FirstToScore { threshold: 0.8 })
        .when(NeedsToMoveScorer, MoveToChunkAction)
        .when(HungerScorer, EatAction)
        .when(ThirstScorer, DrinkAction)
        .when(FatigueScorer, RestAction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::SystemState;

    #[test]
    fn move_to_chunk_action_advances_one_chebyshev_step_per_run() {
        let mut world = World::new();
        let pawn = world
            .spawn((
                Position {
                    chunk: ChunkId(0, 0),
                },
                MovementTarget(ChunkId(3, 0)),
            ))
            .id();

        let action = world
            .spawn((Actor(pawn), MoveToChunkAction, ActionState::Requested))
            .id();

        let mut schedule = Schedule::default();
        schedule.add_systems(move_to_chunk_action_system);

        schedule.run(&mut world);
        assert_eq!(world.entity(pawn).get::<Position>().unwrap().chunk, ChunkId(0, 0));
        assert!(matches!(
            *world.entity(action).get::<ActionState>().unwrap(),
            ActionState::Executing
        ));

        schedule.run(&mut world);
        assert_eq!(world.entity(pawn).get::<Position>().unwrap().chunk, ChunkId(1, 0));

        schedule.run(&mut world);
        assert_eq!(world.entity(pawn).get::<Position>().unwrap().chunk, ChunkId(2, 0));

        schedule.run(&mut world);
        assert_eq!(world.entity(pawn).get::<Position>().unwrap().chunk, ChunkId(3, 0));
        assert!(world.entity(pawn).contains::<MovementTarget>());

        schedule.run(&mut world);
        assert!(!world.entity(pawn).contains::<MovementTarget>());
        assert!(matches!(
            *world.entity(action).get::<ActionState>().unwrap(),
            ActionState::Success
        ));
    }

    #[test]
    fn quest_acceptance_sets_in_progress_provider() {
        let mut world = World::new();
        world.insert_resource(QuestBoard::default());

        let requester = world
            .spawn((
                Name::new("Requester"),
                Position {
                    chunk: ChunkId(0, 0),
                },
                NeuralNetworkComponent {
                    hunger: 0.8,
                    ..default()
                },
                Capabilities {
                    can_do: vec!["Eat".into()],
                },
            ))
            .id();

        let provider = world
            .spawn((
                Name::new("Provider"),
                Position {
                    chunk: ChunkId(1, 0),
                },
                NeuralNetworkComponent {
                    hunger: 0.1,
                    ..default()
                },
                Capabilities {
                    can_do: vec!["Eat".into()],
                },
            ))
            .id();

        {
            let mut board = world.resource_mut::<QuestBoard>();
            board.active_quests.push(Quest {
                need: "food".to_string(),
                requester,
                chunk: ChunkId(0, 0),
                provider: None,
                status: QuestStatus::Open,
            });
        }

        let mut system_state: SystemState<(
            ResMut<QuestBoard>,
            Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
            Query<&Capabilities>,
        )> = SystemState::new(&mut world);

        {
            let (mut quest_board, mut pawn_query, capabilities_query) =
                system_state.get_mut(&mut world);
            quest_acceptance_step(
                1,
                &mut quest_board,
                &mut pawn_query,
                &capabilities_query,
                None,
                None,
                false,
            );
        }
        system_state.apply(&mut world);

        let board = world.resource::<QuestBoard>();
        assert_eq!(board.active_quests.len(), 1);
        let quest = &board.active_quests[0];
        assert_eq!(quest.provider, Some(provider));
        assert!(matches!(
            quest.status,
            QuestStatus::InProgress { provider: p } if p == provider
        ));
    }
}
