use bevy::prelude::*;
use bevy::ecs::message::{MessageReader, Messages};
use bevy::input::mouse::MouseWheel;
use bevy::sprite::Sprite;
use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use simrard_lib_utility_ai::{BigBrainPlugin, BigBrainSet};
use simrard_lib_ai::{self as ai, build_pawn_brain, ActivityLog, PawnAIPlugin};
use simrard_lib_causal::{
    heartbeat, chebyshev_distance, propagation_delay, CausalEventKind, CausalEventQueue,
    CausalPlugin,
};
use simrard_lib_charter::{
    CharterFlashEvent, ChunkId, FrameWriteLog, LeaseHandle,
    LeaseIntent, SpatialCharter, SpatialLease,
};
#[cfg(debug_assertions)]
use simrard_lib_charter::charter_watchguard_system;
use simrard_lib_hypergraph::HypergraphSubstrate;
use simrard_lib_pawn::{
    Capabilities, FoodReservation, ItemHistory, ItemIdAllocator, ItemIdentity, KnownRecipes,
    MortalityCause, NeuralNetworkComponent, Position, Quest, QuestBoard, QuestStatus, RestSpot,
    SimulationLogSettings, SimulationReport, WaterSource, WORLD_CHUNK_EXTENT,
};
use simrard_lib_time::{
    CausalClock, GlobalTickClock, SimTickAccumulator, SimTimeScale, TimePlugin,
    SIM_TICKS_PER_SECOND_AT_1X,
};
use simrard_lib_transforms::TransformsPlugin;
use simrard_lib_mirror::{push_ecs_snapshot_system, MirrorPlugin};

const HEADLESS_TARGET_TICKS: u64 = 10_000;
const HEADLESS_MAX_WALL_SECONDS: f64 = 60.0;
const HEADLESS_MIN_SURVIVORS: usize = 8;

static HEADLESS_PERF: OnceLock<Mutex<PerfAudit>> = OnceLock::new();
static TIER10_ENABLED: OnceLock<bool> = OnceLock::new();
static HEADLESS_SUBSTRATE: OnceLock<bool> = OnceLock::new();

#[derive(Default)]
struct PerfAudit {
    totals: HashMap<&'static str, Duration>,
    counts: HashMap<&'static str, u64>,
    headless_total: Duration,
    headless_updates: u64,
}

fn perf_audit() -> &'static Mutex<PerfAudit> {
    HEADLESS_PERF.get_or_init(|| Mutex::new(PerfAudit::default()))
}

fn perf_reset() {
    if let Ok(mut audit) = perf_audit().lock() {
        *audit = PerfAudit::default();
    }
}

fn perf_record(name: &'static str, elapsed: Duration) {
    if let Ok(mut audit) = perf_audit().lock() {
        *audit.totals.entry(name).or_insert(Duration::ZERO) += elapsed;
        *audit.counts.entry(name).or_insert(0) += 1;
    }
}

fn perf_record_headless_frame(elapsed: Duration) {
    if let Ok(mut audit) = perf_audit().lock() {
        audit.headless_total += elapsed;
        audit.headless_updates += 1;
    }
}

fn tier10_enabled_from_args() -> bool {
    *TIER10_ENABLED.get_or_init(|| !std::env::args().skip(1).any(|arg| arg == "--disable-tier10"))
}

fn headless_substrate_from_args() -> bool {
    *HEADLESS_SUBSTRATE.get_or_init(|| std::env::args().skip(1).any(|arg| arg == "--headless-substrate"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SimulationMode {
    Interactive,
    Headless,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeadlessProfile {
    Full,
    SubstrateOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HeadlessTermination {
    TickLimitReached,
    WallTimeLimitReached,
    AllPawnsDied,
    Panic(String),
}

#[derive(Debug, Clone)]
struct HeadlessRunResult {
    termination: HeadlessTermination,
    report: String,
}

pub fn run() {
    match parse_mode() {
        SimulationMode::Interactive => run_interactive(),
        SimulationMode::Headless => {
            let result = if headless_substrate_from_args() {
                run_headless_substrate()
            } else {
                run_headless()
            };
            println!("{}", result.report);
            if matches!(result.termination, HeadlessTermination::Panic(_)) {
                std::process::exit(1);
            }
        }
    }
}

fn parse_mode() -> SimulationMode {
    if std::env::args().skip(1).any(|arg| arg == "--headless-test" || arg == "--headless-substrate") {
        SimulationMode::Headless
    } else {
        SimulationMode::Interactive
    }
}

fn run_interactive() {
    let mut app = App::new();
    app.set_error_handler(bevy::ecs::error::panic);
    app.add_plugins(DefaultPlugins)
        .add_plugins(BigBrainPlugin::new(PreUpdate))
        .add_plugins(TransformsPlugin)
        .add_plugins(PawnAIPlugin)
        .add_plugins(TimePlugin)
        .add_plugins(CausalPlugin)
        .init_resource::<QuestBoard>()
        .init_resource::<ItemIdAllocator>()
        .init_resource::<SpatialCharter>()
        .init_resource::<FrameWriteLog>()
        .init_resource::<Messages<CharterFlashEvent>>()
        .init_resource::<ActivityLog>()
        .init_resource::<HypergraphSubstrate>()
        .init_resource::<ChemistryState>()
        .init_resource::<RespawnState>()
        .init_resource::<SimLifeState>()
        .add_plugins(MirrorPlugin);
    #[cfg(debug_assertions)]
    app.init_resource::<HypergraphDebugViz>();
    // Alpha: panic on any ECS error in every Bevy world, including plugin-created sub-apps.
    // Some plugins create or reconfigure sub-app worlds during build, so overwrite all handlers
    // only after plugin registration is complete and before any schedule runs.
    force_panic_error_handlers(&mut app);
    app.add_systems(Startup, (setup, setup_quest_ui))
        // Phase 4.0: Run sim tick in PreUpdate before BigBrain so each frame we advance tick then run
        // scorers/thinker/actions (including MoveToChunk). Movement and sim state stay in sync.
        // Phase 4.1: Death first so no system ever sees or queues commands for dead pawns.
        // Order: despawn dead (hunger/thirst <= 0) → flush → tick driver (dispatcher, heartbeat)
        // → respawn → BigBrain. Otherwise dispatcher/quest_board or BigBrain can hold Entity refs
        // that get despawned later and cause "Entity despawned" command errors.
        .add_systems(
            PreUpdate,
            (
                ai::pawn_death_system,
                ApplyDeferred,
                sim_tick_driver,
                hypergraph_tick_system,
                simlife_tick_system,
                curiosity_discovery_system,
                resource_respawn_system,
            )
                .chain()
                .before(BigBrainSet::Scorers),
        )
        .add_systems(
            Update,
            (
                sync_position_to_transform,
                time_scale_input,
                camera_pan_zoom_input,
                chunk_grid_gizmo_system,
                resource_level_bar_system,
                pawn_dominant_drive_color_system,
                charter_flash_spawn_system,
                charter_flash_tick_system,
                hypergraph_debug_input_system,
                ui_panel_update_system,
                hypergraph_debug_viz_system,
            )
                .chain(),
        );

    // Phase 4.D1: Push ECS snapshot to DuckDB mirror after Update (post visual sync).
    // Runs in its own add_systems call so it is not constrained by the Update chain tuple limit.
    app.add_systems(Update, push_ecs_snapshot_system);

    #[cfg(debug_assertions)]
    app.add_systems(Update, charter_watchguard_system);

    app.run();
}

fn main() {
    run();
}

fn run_headless() -> HeadlessRunResult {
    run_headless_with_target_ticks(HEADLESS_TARGET_TICKS)
}

fn run_headless_substrate() -> HeadlessRunResult {
    run_headless_with_profile(HEADLESS_TARGET_TICKS, HeadlessProfile::SubstrateOnly)
}

fn run_headless_with_target_ticks(target_ticks: u64) -> HeadlessRunResult {
    run_headless_with_profile(target_ticks, HeadlessProfile::Full)
}

fn run_headless_with_profile(target_ticks: u64, profile: HeadlessProfile) -> HeadlessRunResult {
    perf_reset();
    let tier10_enabled = tier10_enabled_from_args();

    let mut app = App::new();
    app.set_error_handler(bevy::ecs::error::panic);
    app.add_plugins(MinimalPlugins)
        .add_plugins(TimePlugin)
        .add_plugins(CausalPlugin)
        .init_resource::<ItemIdAllocator>()
        .init_resource::<SpatialCharter>()
        .init_resource::<FrameWriteLog>()
        .init_resource::<Messages<CharterFlashEvent>>()
        .init_resource::<ActivityLog>()
        .init_resource::<HypergraphSubstrate>()
        .init_resource::<ChemistryState>()
        .init_resource::<RespawnState>()
        .init_resource::<SimLifeState>()
        .init_resource::<SimulationReport>()
        .insert_resource(SimulationLogSettings { stdout_enabled: false });

    match profile {
        HeadlessProfile::Full => {
            app.add_plugins(BigBrainPlugin::new(PreUpdate))
                .add_plugins(PawnAIPlugin)
                .init_resource::<QuestBoard>()
                .add_plugins(MirrorPlugin)
                .add_systems(Startup, (setup, initialize_report_baseline).chain())
                .add_systems(
                    PreUpdate,
                    (
                        ai::pawn_death_system,
                        ApplyDeferred,
                        headless_tick_driver,
                        hypergraph_tick_system,
                        simlife_tick_system,
                        curiosity_discovery_system,
                        resource_respawn_system,
                    )
                        .chain()
                        .before(BigBrainSet::Scorers),
                )
                .add_systems(Update, push_ecs_snapshot_system);
        }
        HeadlessProfile::SubstrateOnly => {
            app.init_resource::<QuestBoard>()
                .init_resource::<SubstrateStabilityState>()
                .add_systems(Startup, initialize_substrate_baseline)
                .add_systems(
                    PreUpdate,
                    (
                        substrate_tick_driver,
                        hypergraph_tick_system,
                        simlife_tick_system,
                        substrate_stability_probe_system,
                    )
                        .chain(),
                );
        }
    }

    force_panic_error_handlers(&mut app);

    let started = Instant::now();
    let termination = loop {
        let frame_started = Instant::now();
        let update_result = catch_unwind(AssertUnwindSafe(|| app.update()));
        perf_record_headless_frame(frame_started.elapsed());
        match update_result {
            Ok(()) => {
                let tick = app.world().resource::<GlobalTickClock>().causal_seq();
                if tick >= target_ticks {
                    break HeadlessTermination::TickLimitReached;
                }
                if profile == HeadlessProfile::Full && started.elapsed().as_secs_f64() >= HEADLESS_MAX_WALL_SECONDS {
                    break HeadlessTermination::WallTimeLimitReached;
                }
                if profile == HeadlessProfile::Full && count_living_pawns(app.world_mut()) == 0 {
                    break HeadlessTermination::AllPawnsDied;
                }
            }
            Err(payload) => {
                break HeadlessTermination::Panic(panic_payload_to_string(payload));
            }
        }
    };

    let living_at_end = count_living_pawns(app.world_mut());
    let guarded_termination = match termination {
        HeadlessTermination::TickLimitReached | HeadlessTermination::WallTimeLimitReached
            if profile == HeadlessProfile::Full && living_at_end < HEADLESS_MIN_SURVIVORS =>
        {
            HeadlessTermination::Panic(format!(
                "survival regression: {} living pawns at end, require at least {}",
                living_at_end, HEADLESS_MIN_SURVIVORS
            ))
        }
        _ => termination,
    };

    let report = build_headless_report(
        &mut app,
        &guarded_termination,
        started.elapsed().as_secs_f64(),
        tier10_enabled,
        profile,
    );
    HeadlessRunResult {
        termination: guarded_termination,
        report,
    }
}

fn force_panic_error_handlers(app: &mut App) {
    for sub_app in app.sub_apps_mut().iter_mut() {
        sub_app.insert_resource(bevy::ecs::error::DefaultErrorHandler(bevy::ecs::error::panic));
    }
}

fn initialize_report_baseline(
    pawn_query: Query<(), With<NeuralNetworkComponent>>,
    mut report: Option<ResMut<SimulationReport>>,
) {
    if let Some(ref mut report) = report {
        report.set_initial_pawn_count(pawn_query.iter().count());
    }
}

fn initialize_substrate_baseline(mut report: Option<ResMut<SimulationReport>>) {
    if let Some(ref mut report) = report {
        report.set_initial_pawn_count(0);
    }
}

fn substrate_tick_driver(
    mut global_clock: ResMut<GlobalTickClock>,
    mut report: Option<ResMut<SimulationReport>>,
) {
    let started = Instant::now();
    global_clock.increment();
    if let Some(ref mut report) = report {
        report.bump("sim_ticks_advanced");
    }
    perf_record("headless_tick_driver", started.elapsed());
}

#[derive(Resource, Debug, Clone)]
struct SubstrateStabilityState {
    last_tick: u64,
    start_coverage_pct: Option<f32>,
    end_coverage_pct: Option<f32>,
    start_histogram: Option<[u64; 8]>,
    end_histogram: Option<[u64; 8]>,
}

impl Default for SubstrateStabilityState {
    fn default() -> Self {
        Self {
            last_tick: 0,
            start_coverage_pct: None,
            end_coverage_pct: None,
            start_histogram: None,
            end_histogram: None,
        }
    }
}

fn substrate_stability_probe_system(
    global_clock: Res<GlobalTickClock>,
    simlife: Res<SimLifeState>,
    chemistry: Res<ChemistryState>,
    mut state: ResMut<SubstrateStabilityState>,
) {
    let seq = global_clock.causal_seq();
    if seq <= state.last_tick {
        return;
    }
    state.last_tick = seq;

    let total_cells = ((CHUNK_EXTENT as u64) + 1).pow(2) as f32;
    let active_cells = simlife
        .grass_per_chunk
        .values()
        .filter(|value| **value > 0)
        .count() as f32;
    let coverage_pct = if total_cells > 0.0 {
        (active_cells / total_cells) * 100.0
    } else {
        0.0
    };

    let mut histogram = [0_u64; 8];
    for value in chemistry.receptor_noise_floor_by_chunk.values() {
        let normalized = (*value / HYPERGRAPH_NOISE_FLOOR_MAX).clamp(0.0, 0.9999);
        let bin = (normalized * histogram.len() as f32).floor() as usize;
        histogram[bin] += 1;
    }

    if state.start_coverage_pct.is_none() {
        state.start_coverage_pct = Some(coverage_pct);
    }
    if state.start_histogram.is_none() {
        state.start_histogram = Some(histogram);
    }
    state.end_coverage_pct = Some(coverage_pct);
    state.end_histogram = Some(histogram);
}

fn advance_simulation_one_tick(
    global_clock: &mut GlobalTickClock,
    event_queue: &mut CausalEventQueue,
    quest_board: &mut QuestBoard,
    activity: &mut ActivityLog,
    evaluation_state: &mut ai::DispatcherEvaluationState,
    pawn_query: &mut Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
    capabilities_query: &Query<&Capabilities>,
    known_recipes_query: &mut Query<&mut KnownRecipes>,
    food_query: &Query<(&Position, &FoodReservation)>,
    water_query: &Query<(&Position, &WaterSource)>,
    hypergraph: &HypergraphSubstrate,
    report: Option<&mut SimulationReport>,
    stdout_enabled: bool,
) {
    let started = Instant::now();
    global_clock.increment();
    perf_record("causal_clock_increment", started.elapsed());

    let heartbeat_started = Instant::now();
    let seq = global_clock.causal_seq();
    let mut report = report;
    heartbeat::drive_decay_heartbeat_pulse(
        seq,
        pawn_query,
        event_queue,
        report.as_deref_mut(),
        stdout_enabled,
    );
    perf_record("heartbeat_decay", heartbeat_started.elapsed());

    let mut region_signatures: HashMap<Entity, u64> = HashMap::new();
    for (entity, _, position, _) in pawn_query.iter_mut() {
        let food_sum: u64 = food_query
            .iter()
            .filter(|(food_pos, _)| food_pos.chunk == position.chunk)
            .map(|(_, food)| food.portions as u64)
            .sum();
        let water_sum: u64 = water_query
            .iter()
            .filter(|(water_pos, _)| water_pos.chunk == position.chunk)
            .map(|(_, water)| water.portions as u64)
            .sum();
        let (clustering_q, causal_q) = match hypergraph.output_for_chunk(position.chunk.0, position.chunk.1) {
            Some(output) => (
                (output.clustering.clamp(0.0, 1.0) * 1024.0).round() as u64,
                (output.causal_volume.clamp(0.0, 1.0) * 1024.0).round() as u64,
            ),
            None => (0, 0),
        };

        let signature = (position.chunk.0 as u64)
            | ((position.chunk.1 as u64) << 10)
            | ((food_sum & 0x3ff) << 20)
            | ((water_sum & 0x3ff) << 30)
            | ((clustering_q & 0x7ff) << 40)
            | ((causal_q & 0x1fff) << 51);
        region_signatures.insert(entity, signature);
    }

    let dispatch_started = Instant::now();
    ai::pawn_event_dispatcher_step(
        seq,
        event_queue,
        quest_board,
        pawn_query,
        capabilities_query,
        known_recipes_query,
        &region_signatures,
        Some(activity),
        report.as_deref_mut(),
        evaluation_state,
        stdout_enabled,
    );
    perf_record("pawn_event_dispatcher", dispatch_started.elapsed());

    let bump_started = Instant::now();
    if let Some(report) = report.as_deref_mut() {
        report.bump("sim_ticks_advanced");
    }
    perf_record("sim_tick_counter", bump_started.elapsed());
}

fn headless_tick_driver(
    mut global_clock: ResMut<GlobalTickClock>,
    mut event_queue: ResMut<CausalEventQueue>,
    mut quest_board: ResMut<QuestBoard>,
    mut activity: ResMut<ActivityLog>,
    mut evaluation_state: ResMut<ai::DispatcherEvaluationState>,
    mut pawn_query: Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
    capabilities_query: Query<&Capabilities>,
    mut known_recipes_query: Query<&mut KnownRecipes>,
    food_query: Query<(&Position, &FoodReservation)>,
    water_query: Query<(&Position, &WaterSource)>,
    hypergraph: Res<HypergraphSubstrate>,
    mut report: Option<ResMut<SimulationReport>>,
    log_settings: Option<Res<SimulationLogSettings>>,
) {
    let started = Instant::now();
    advance_simulation_one_tick(
        &mut global_clock,
        &mut event_queue,
        &mut quest_board,
        &mut activity,
        &mut evaluation_state,
        &mut pawn_query,
        &capabilities_query,
        &mut known_recipes_query,
        &food_query,
        &water_query,
        &hypergraph,
        report.as_deref_mut(),
        log_settings
            .as_deref()
            .map(|settings| settings.stdout_enabled)
            .unwrap_or(true),
    );
    perf_record("headless_tick_driver", started.elapsed());
}

fn count_living_pawns(world: &mut World) -> usize {
    let mut pawn_query = world.query::<&NeuralNetworkComponent>();
    pawn_query.iter(world).count()
}

fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_string(),
            Err(_) => "non-string panic payload".to_string(),
        },
    }
}

fn build_headless_report(
    app: &mut App,
    termination: &HeadlessTermination,
    elapsed_secs: f64,
    tier10_enabled: bool,
    profile: HeadlessProfile,
) -> String {
    let world = app.world_mut();
    let tick = world.resource::<GlobalTickClock>().causal_seq();
    let report = world.resource::<SimulationReport>().clone();
    let activity_entries: Vec<String> = world.resource::<ActivityLog>().0.iter().cloned().collect();
    let living_pawns = count_living_pawns(world);

    let mut food_query = world.query::<&FoodReservation>();
    let food_entities = food_query.iter(world).count();
    let food_portions: u32 = food_query.iter(world).map(|food| food.portions).sum();

    let mut water_query = world.query::<&WaterSource>();
    let water_entities = water_query.iter(world).count();
    let water_portions: u32 = water_query.iter(world).map(|water| water.portions).sum();

    let active_quests = world.resource::<QuestBoard>().active_quests.len();
    let ticks_per_second = if elapsed_secs > 0.0 {
        tick as f64 / elapsed_secs
    } else {
        0.0
    };

    let termination_line = match termination {
        HeadlessTermination::TickLimitReached => {
            format!("Tick limit reached at {} ticks.", tick)
        }
        HeadlessTermination::WallTimeLimitReached => {
            format!("Wall-time limit reached at {:.1}s and {} ticks.", elapsed_secs, tick)
        }
        HeadlessTermination::AllPawnsDied => {
            format!("All pawns died by tick {}.", tick)
        }
        HeadlessTermination::Panic(message) => {
            format!("Simulation panicked at tick {}: {}", tick, message)
        }
    };

    let mut lines = vec![
        "=== Simrard Headless Report ===".to_string(),
        termination_line,
        format!(
            "Runtime: {:.3}s wall-clock, {:.1} ticks/sec.",
            elapsed_secs, ticks_per_second
        ),
        format!(
            "Tier10: {} (use --disable-tier10 for occasional regression checks).",
            if tier10_enabled { "enabled" } else { "disabled" }
        ),
        format!(
            "Profile: {}.",
            match profile {
                HeadlessProfile::Full => "full",
                HeadlessProfile::SubstrateOnly => "substrate-only",
            }
        ),
        format!(
            "Pawns: {} alive / {} started / {} dead.",
            living_pawns,
            report.initial_pawn_count,
            report.initial_pawn_count.saturating_sub(living_pawns)
        ),
        format!(
            "Resources: food {} entities / {} portions, water {} entities / {} portions.",
            food_entities, food_portions, water_entities, water_portions
        ),
        format!("Open quests: {}.", active_quests),
    ];

    let deaths_hunger = report
        .death_records
        .iter()
        .filter(|record| matches!(record.cause, MortalityCause::Hunger))
        .count();
    let deaths_thirst = report
        .death_records
        .iter()
        .filter(|record| matches!(record.cause, MortalityCause::Thirst))
        .count();
    let deaths_other = report
        .death_records
        .iter()
        .filter(|record| matches!(record.cause, MortalityCause::Other))
        .count();
    lines.push("Mortality Report:".to_string());
    lines.push(format!(
        "  Deaths by cause: Hunger {} / Thirst {} / Other {}.",
        deaths_hunger, deaths_thirst, deaths_other
    ));
    lines.push(format!(
        "  Total deaths recorded: {}.",
        report.death_records.len()
    ));
    if !report.death_records.is_empty() {
        lines.push("  Death details: ".to_string());
        for death in report.death_records.iter().take(8) {
            lines.push(format!(
                "    tick {}: {} cause={:?} primary={} drives[h={:.3}, t={:.3}, f={:.3}, c={:.3}, s={:.3}, fear={:.3}, ind={:.3}, comfort={:.3}] @ {:?}",
                death.tick,
                death.pawn_name,
                death.cause,
                death.primary_drive,
                death.hunger,
                death.thirst,
                death.fatigue,
                death.curiosity,
                death.social,
                death.fear,
                death.industriousness,
                death.comfort,
                death.chunk,
            ));
        }
    }

    if !report.counters.is_empty() {
        lines.push("Counters:".to_string());
        for (key, value) in &report.counters {
            lines.push(format!("  {} = {}", key, value));
        }
    }

    if !report.notable_events.is_empty() {
        lines.push("Notable events:".to_string());
        for event in &report.notable_events {
            lines.push(format!("  {}", event));
        }
    }

    if !activity_entries.is_empty() {
        lines.push("Recent activity:".to_string());
        for entry in activity_entries.iter().rev().take(10) {
            lines.push(format!("  {}", entry));
        }
    }

    if profile == HeadlessProfile::SubstrateOnly {
        if let Some(stability) = world.get_resource::<SubstrateStabilityState>() {
            let start = match stability.start_coverage_pct {
                Some(value) => value,
                None => 0.0,
            };
            let end = match stability.end_coverage_pct {
                Some(value) => value,
                None => 0.0,
            };
            let growth = end - start;

            let start_hist = match stability.start_histogram {
                Some(value) => value,
                None => [0; 8],
            };
            let end_hist = match stability.end_histogram {
                Some(value) => value,
                None => [0; 8],
            };
            let l1_delta: u64 = start_hist
                .iter()
                .zip(end_hist.iter())
                .map(|(a, b)| a.abs_diff(*b))
                .sum();
            let end_total: u64 = end_hist.iter().sum();
            let stability_score = if end_total == 0 {
                1.0
            } else {
                1.0 - (l1_delta as f64 / (2.0 * end_total as f64)).clamp(0.0, 1.0)
            };

            lines.push("Substrate Stability:".to_string());
            lines.push(format!(
                "  fungal coverage: start {:.4}% -> end {:.4}% (delta {:+.4}%).",
                start, end, growth
            ));
            lines.push(format!(
                "  chemistry histogram stability (L1-normalized): {:.4} (1.0 is perfectly stable).",
                stability_score
            ));
            lines.push(format!(
                "  chemistry histogram end bins: {:?}.",
                end_hist
            ));
        }
    }

    if let Ok(audit) = perf_audit().lock() {
        let mut entries: Vec<(&str, Duration, u64)> = audit
            .totals
            .iter()
            .map(|(name, total)| {
                let calls = *audit
                    .counts
                    .get(name)
                    .expect("perf count missing for total; fix perf accounting invariants");
                (*name, *total, calls)
            })
            .collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1));

        if !entries.is_empty() {
            lines.push("Perf breakdown (top 3):".to_string());
            let denom = audit.headless_total.as_secs_f64().max(0.000_001);
            for (name, total, count) in entries.into_iter().take(3) {
                let total_ms = total.as_secs_f64() * 1000.0;
                let pct = (total.as_secs_f64() / denom) * 100.0;
                let avg_ms = if count == 0 {
                    0.0
                } else {
                    total_ms / count as f64
                };
                lines.push(format!(
                    "  {}: {:.1}% ({:.2} ms total, {:.4} ms avg over {} calls)",
                    name, pct, total_ms, avg_ms, count
                ));
            }
            lines.push(format!(
                "  headless_total: {:.2} ms over {} updates",
                audit.headless_total.as_secs_f64() * 1000.0,
                audit.headless_updates
            ));
        }
    }

    let event_collection_us = match report.counters.get("dispatcher_event_collection_us") {
        Some(value) => *value,
        None => 0,
    };
    let lease_requests_us = match report.counters.get("dispatcher_lease_requests_us") {
        Some(value) => *value,
        None => 0,
    };
    let per_pawn_score_collection_us = match report.counters.get("dispatcher_per_pawn_score_collection_us") {
        Some(value) => *value,
        None => 0,
    };
    let biochemical_lookup_us = match report.counters.get("dispatcher_biochemical_base_lookup_us") {
        Some(value) => *value,
        None => 0,
    };
    let contextual_modifier_us = match report.counters.get("dispatcher_contextual_modifier_us") {
        Some(value) => *value,
        None => 0,
    };
    let score_combine_sort_us = match report.counters.get("dispatcher_score_combine_sort_us") {
        Some(value) => *value,
        None => 0,
    };
    let winner_selection_us = match report.counters.get("dispatcher_winner_selection_us") {
        Some(value) => *value,
        None => 0,
    };

    let mut dispatcher_phases: Vec<(&str, u64)> = vec![
        ("event collection", event_collection_us),
        ("per-pawn score collection", per_pawn_score_collection_us),
        ("biochemical base lookup", biochemical_lookup_us),
        ("contextual modifier application", contextual_modifier_us),
        ("final score combine+sorting", score_combine_sort_us),
        ("winner selection+action prep", winner_selection_us),
        ("lease requests", lease_requests_us),
    ];
    dispatcher_phases.retain(|(_, us)| *us > 0);
    if !dispatcher_phases.is_empty() {
        dispatcher_phases.sort_by(|a, b| b.1.cmp(&a.1));
        let total_us: u64 = dispatcher_phases.iter().map(|(_, us)| *us).sum();
        let denom = (total_us as f64).max(1.0);
        lines.push("Dispatcher internals (phase breakdown):".to_string());
        for (phase, total_us) in &dispatcher_phases {
            let total_ms = *total_us as f64 / 1000.0;
            let pct = (*total_us as f64 / denom) * 100.0;
            lines.push(format!(
                "  {}: {:.1}% ({:.2} ms total)",
                phase, pct, total_ms
            ));
        }
    }

    lines.join("\n")
}

// ---- Phase 3.5: Chunk grid ----
fn chunk_grid_gizmo_system(mut gizmos: Gizmos) {
    let extent = CHUNK_EXTENT + 1; // Draw boundaries for chunks in 0..=CHUNK_EXTENT.
    let color = Color::srgba(0.3, 0.3, 0.35, 0.6);
    for i in 0..=extent {
        let p = i as f32 * CHUNK_PIXEL;
        gizmos.line_2d(Vec2::new(p, 0.0), Vec2::new(p, extent as f32 * CHUNK_PIXEL), color);
        gizmos.line_2d(Vec2::new(0.0, p), Vec2::new(extent as f32 * CHUNK_PIXEL, p), color);
    }
}

// ---- Position ↔ visual ----
// Syncs Position (simulation) to Transform (render). Runs after sim_tick_driver and pawn_wander_system
// so the same frame we advance the sim we push state to the visual (no one-frame delay).
// Pawns use DisplayOffset so multiple pawns on the same chunk don't stack; other entities use chunk center.
#[derive(Component, Clone, Copy)]
struct DisplayOffset(pub Vec3);

#[derive(Component)]
struct ResourceLevelBarVisual;

fn sync_position_to_transform(
    mut query: Query<(&Position, &mut Transform, Option<&DisplayOffset>), With<Sprite>>,
) {
    for (position, mut transform, offset) in query.iter_mut() {
        let base = chunk_to_translation(&position.chunk, transform.translation.z);
        let delta = offset.map(|o| o.0).unwrap_or(Vec3::ZERO);
        transform.translation = base + delta;
    }
}

// ---- Phase 3.5: Pawn color by dominant drive ----
#[derive(Component)]
struct PawnVisual;

fn pawn_dominant_drive_color_system(
    mut query: Query<(&mut Sprite, &NeuralNetworkComponent), With<PawnVisual>>,
) {
    for (mut sprite, nn) in query.iter_mut() {
        // Use distinct colors so pawns are never confused with water (cyan) or food (orange).
        let (r, g, b) = if nn.hunger <= nn.thirst && nn.hunger <= nn.fatigue {
            (0.9, 0.3, 0.2) // hunger dominant -> red
        } else if nn.thirst <= nn.fatigue {
            (0.55, 0.25, 0.9) // thirst -> purple (water is cyan)
        } else {
            (0.5, 0.5, 0.4) // fatigue -> gray/yellow
        };
        sprite.color = Color::srgb(r, g, b);
    }
}

// ---- Phase 3.5: Charter grant/deny flash ----
#[derive(Component)]
struct CharterFlashOverlay(Timer);

fn charter_flash_spawn_system(
    mut commands: Commands,
    mut reader: MessageReader<CharterFlashEvent>,
) {
    for ev in reader.read() {
        let pos = chunk_to_translation(&ev.chunk, 0.5);
        let color = if ev.granted {
            Color::srgba(0.2, 0.8, 0.3, 0.4)
        } else {
            Color::srgba(0.9, 0.2, 0.2, 0.4)
        };
        commands.spawn((
            Sprite::from_color(color, Vec2::splat(CHUNK_PIXEL - 2.0)),
            Transform::from_translation(pos),
            CharterFlashOverlay(Timer::from_seconds(0.2, TimerMode::Once)),
        ));
    }
}

fn charter_flash_tick_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut CharterFlashOverlay)>,
) {
    for (entity, mut overlay) in query.iter_mut() {
        overlay.0.tick(time.delta());
        if overlay.0.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

// ---- UI overlay: sim status, legend, quests, activity feed ----
#[derive(Component)]
struct QuestOverlayRoot;

const UI_SECTIONS: usize = 5; // sim, legend, resources, quests, activity (order in children)

/// Colors used in the legend and for pawns/resources. Use actual color, not words only.
fn legend_colors() -> (
    Color,
    Color,
    Color,
    Color,
    Color,
    Color,
) {
    (
        Color::srgb(0.9, 0.3, 0.2),   // hunger
        Color::srgb(0.55, 0.25, 0.9), // thirst
        Color::srgb(0.5, 0.5, 0.4),   // fatigue
        Color::srgb(0.9, 0.5, 0.1),   // food
        Color::srgb(0.2, 0.85, 0.95), // water
        Color::srgb(0.88, 0.88, 0.88), // neutral label
    )
}

fn setup_quest_ui(mut commands: Commands) {
    let font = TextFont {
        font_size: 13.0,
        ..default()
    };
    let layout = TextLayout::default();
    let (hunger_c, thirst_c, fatigue_c, food_c, water_c, neutral_c) = legend_colors();
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(8.0),
                top: Val::Px(8.0),
                width: Val::Px(340.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(12.0),
                ..default()
            },
            QuestOverlayRoot,
        ))
        .with_children(|parent| {
            // Section 0: sim status (plain text, updated each frame)
            parent.spawn((
                Text::new(""),
                font.clone(),
                layout.clone(),
            ));
            // Section 1: legend — colored text only (no color words); never overwritten
            parent
                .spawn((
                    Text::default(),
                    font.clone(),
                    layout.clone(),
                ))
                .with_children(|p| {
                    p.spawn((TextSpan::new("Pawn color = dominant need:\n  "), TextColor(neutral_c.into())));
                    p.spawn((TextSpan::new("hunger"), TextColor(hunger_c.into())));
                    p.spawn((TextSpan::new("   "), TextColor(neutral_c.into())));
                    p.spawn((TextSpan::new("thirst"), TextColor(thirst_c.into())));
                    p.spawn((TextSpan::new("   "), TextColor(neutral_c.into())));
                    p.spawn((TextSpan::new("fatigue"), TextColor(fatigue_c.into())));
                    p.spawn((TextSpan::new("\n  Big "), TextColor(neutral_c.into())));
                    p.spawn((TextSpan::new("food"), TextColor(food_c.into())));
                    p.spawn((TextSpan::new("   "), TextColor(neutral_c.into())));
                    p.spawn((TextSpan::new("water"), TextColor(water_c.into())));
                });
            // Sections 2–4: resources, quests, activity (plain text, updated each frame)
            for _ in 0..(UI_SECTIONS - 2) {
                parent.spawn((
                    Text::new(""),
                    font.clone(),
                    layout.clone(),
                ));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::{
        advance_simlife_grass, food_portions_from_grass, force_panic_error_handlers,
        run_headless_with_target_ticks, HeadlessTermination, SimLifeState,
        HEADLESS_SURVIVAL_BASELINE_TICK, SIMLIFE_GRASS_MAX,
    };
    use bevy::app::{AppLabel, SubApp};
    use bevy::prelude::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AppLabel)]
    struct TestSubApp;

    #[test]
    fn force_panic_error_handlers_overwrites_all_sub_apps() {
        let mut app = App::new();
        app.insert_resource(bevy::ecs::error::DefaultErrorHandler(bevy::ecs::error::warn));

        let mut sub_app = SubApp::new();
        sub_app.insert_resource(bevy::ecs::error::DefaultErrorHandler(bevy::ecs::error::warn));
        app.insert_sub_app(TestSubApp, sub_app);

        force_panic_error_handlers(&mut app);

        let panic_handler = bevy::ecs::error::panic as *const () as usize;
        assert_eq!(app.world().default_error_handler() as usize, panic_handler);
        assert_eq!(
            app.sub_app(TestSubApp).world().default_error_handler() as usize,
            panic_handler
        );
    }

    #[test]
    fn headless_run_emits_distilled_report() {
        let result = run_headless_with_target_ticks(1);

        assert_eq!(result.termination, HeadlessTermination::TickLimitReached);
        assert!(result.report.contains("=== Simrard Headless Report ==="));
        assert!(result.report.contains("Pawns: 8 alive / 8 started / 0 dead."));
        assert!(result.report.contains("sim_ticks_advanced = 1"));
    }

    #[test]
    fn headless_defaults_keep_population_alive_to_staged_baseline_tick() {
        let result = run_headless_with_target_ticks(HEADLESS_SURVIVAL_BASELINE_TICK);
        assert_eq!(result.termination, HeadlessTermination::TickLimitReached);
    }

    #[test]
    fn simlife_grass_advances_over_ticks() {
        let mut state = SimLifeState::default();
        advance_simlife_grass(1, &mut state);
        let sum_after_first: u32 = state.grass_per_chunk.values().copied().sum();
        advance_simlife_grass(25, &mut state);
        let sum_after_second: u32 = state.grass_per_chunk.values().copied().sum();

        assert!(sum_after_second >= sum_after_first);
        assert!(state.grass_per_chunk.values().all(|v| *v <= SIMLIFE_GRASS_MAX));
    }

    #[test]
    fn food_portions_increase_with_grass_signal() {
        let low = food_portions_from_grass(0);
        let high = food_portions_from_grass(8);
        assert!(high > low);
    }
}

fn ui_panel_update_system(
    global_clock: Res<GlobalTickClock>,
    scale: Res<SimTimeScale>,
    hypergraph: Res<HypergraphSubstrate>,
    #[cfg(debug_assertions)] hypergraph_viz: Res<HypergraphDebugViz>,
    quest_board: Res<QuestBoard>,
    activity: Res<ActivityLog>,
    pawn_names: Query<&Name>,
    food_query: Query<(&Position, &FoodReservation)>,
    water_query: Query<(&Position, &WaterSource)>,
    mut writer: bevy::ui::widget::TextUiWriter,
    overlay_query: Query<&Children, With<QuestOverlayRoot>>,
) {
    let Some(children) = overlay_query.iter().next() else { return };
    if children.len() < UI_SECTIONS {
        return;
    }
    let seq = global_clock.causal_seq();
    let pause = if scale.0 == 0.0 { " [PAUSED]" } else { "" };
    #[cfg(debug_assertions)]
    let hypergraph_controls = if hypergraph_viz.enabled {
        "J/K chaos  V viz:on"
    } else {
        "J/K chaos  V viz:off"
    };
    #[cfg(not(debug_assertions))]
    let hypergraph_controls = "";
    let sim_status = format!(
        "Sim tick: {}  Speed: {:.2}x{}\nKeys: R reset  [ ] speed  P pause  Arrows/WASD pan  Wheel zoom\nHypergraph chaos: {:.2} {}",
        seq,
        scale.0,
        pause,
        hypergraph.chaos(),
        hypergraph_controls
    );
    // Legend is built once in setup with actual colors (no color words); section 1 is not overwritten.
    let _legend_placeholder = "Pawn color = dominant need:\n  hunger   thirst   fatigue\n  Big food   water";
    let food_info: String = food_query
        .iter()
        .map(|(pos, f)| format!("{:?}({})", pos.chunk, f.portions))
        .collect::<Vec<_>>()
        .join(", ");
    let water_info: String = water_query
        .iter()
        .map(|(pos, w)| format!("{:?}({})", pos.chunk, w.portions))
        .collect::<Vec<_>>()
        .join(", ");
    let resource_lines = format!("Resources:\n  Food {}: {}\n  Water {}: {}", food_query.iter().count(), food_info, water_query.iter().count(), water_info);
    let quest_lines: String = if quest_board.active_quests.is_empty() {
        "Quests: (none)".into()
    } else {
        std::iter::once("Quests:".into())
            .chain(
                quest_board
                    .active_quests
                    .iter()
                    .take(10)
                    .map(|q| {
                        let status = match q.status {
                            QuestStatus::Open => "Open".to_string(),
                            QuestStatus::Completed => "Completed".to_string(),
                            QuestStatus::InProgress { provider } => {
                                let provider_name = pawn_names
                                    .get(provider)
                                    .map(|n| n.to_string())
                                    .unwrap_or_else(|_| format!("{:?}", provider));
                                format!("InProgress({})", provider_name)
                            }
                        };
                        format!("  {} @ {:?} – {}", q.need, q.chunk, status)
                    }),
            )
            .collect::<Vec<_>>()
            .join("\n")
    };
    let activity_lines: String = if activity.0.is_empty() {
        "Activity: (none yet)".into()
    } else {
        std::iter::once("Activity:".into())
            .chain(activity.0.iter().rev().take(8).cloned())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let contents = [
        sim_status,
        _legend_placeholder.to_string(), // section 1 is colored legend from setup; not overwritten
        resource_lines,
        quest_lines,
        activity_lines,
    ];
    for (i, entity) in children.iter().take(UI_SECTIONS).enumerate() {
        if i == 1 {
            continue; // legend is static colored text from setup
        }
        if let Some((_, _, mut text, ..)) = writer.get(entity, 0) {
            if *text != contents[i] {
                *text = contents[i].clone();
            }
        }
    }
}

/// Pixels per chunk for 2D display. Chunk (0,0) at origin; (10,10) at (400, 400).
const CHUNK_PIXEL: f32 = 40.0;
/// Food = large orange so clearly distinct from water and pawns.
const SPRITE_FOOD: f32 = 18.0;
/// Water = medium cyan so clearly distinct from blue/purple thirst pawns.
const SPRITE_WATER: f32 = 14.0;
const SPRITE_PAWN: f32 = 10.0;
const RESOURCE_BAR_HEIGHT: f32 = 3.0;
const RESOURCE_BAR_MAX_WIDTH: f32 = 18.0;
const RESOURCE_BAR_Y_OFFSET: f32 = 13.0;
const RESOURCE_BAR_MAX_PORTIONS: f32 = 8.0;
const CAMERA_PAN_SPEED: f32 = 500.0;
const CAMERA_MIN_ZOOM: f32 = 0.4;
const CAMERA_MAX_ZOOM: f32 = 4.0;
const CAMERA_ZOOM_STEP: f32 = 0.12;
#[cfg(debug_assertions)]
const HYPERGRAPH_DEBUG_CHAOS_STEP: f32 = 0.05;
const HYPERGRAPH_NOISE_FLOOR_MULTIPLIER: f32 = 0.25;
const HYPERGRAPH_NOISE_FLOOR_MAX: f32 = 0.4;
#[cfg(test)]
const HEADLESS_SURVIVAL_BASELINE_TICK: u64 = 500;

fn chunk_to_translation(chunk: &ChunkId, z: f32) -> Vec3 {
    Vec3::new(
        chunk.0 as f32 * CHUNK_PIXEL,
        chunk.1 as f32 * CHUNK_PIXEL,
        z,
    )
}

fn resource_level_bar_system(
    mut commands: Commands,
    existing_bars: Query<Entity, With<ResourceLevelBarVisual>>,
    food_query: Query<(&Position, &FoodReservation)>,
    water_query: Query<(&Position, &WaterSource)>,
) {
    // Keep implementation simple: rebuild tiny bar overlays each frame from current resource state.
    for entity in existing_bars.iter() {
        commands.entity(entity).despawn();
    }

    for (position, food) in food_query.iter() {
        let normalized = (food.portions as f32 / RESOURCE_BAR_MAX_PORTIONS).clamp(0.0, 1.0);
        let width = RESOURCE_BAR_MAX_WIDTH * normalized.max(0.1);
        let mut bar_pos = chunk_to_translation(&position.chunk, 2.0);
        bar_pos.y += RESOURCE_BAR_Y_OFFSET;
        commands.spawn((
            ResourceLevelBarVisual,
            Sprite::from_color(Color::srgb(0.95, 0.8, 0.2), Vec2::new(width, RESOURCE_BAR_HEIGHT)),
            Transform::from_translation(bar_pos),
        ));
    }

    for (position, water) in water_query.iter() {
        let normalized = (water.portions as f32 / RESOURCE_BAR_MAX_PORTIONS).clamp(0.0, 1.0);
        let width = RESOURCE_BAR_MAX_WIDTH * normalized.max(0.1);
        let mut bar_pos = chunk_to_translation(&position.chunk, 2.0);
        bar_pos.y += RESOURCE_BAR_Y_OFFSET;
        commands.spawn((
            ResourceLevelBarVisual,
            Sprite::from_color(Color::srgb(0.45, 0.9, 1.0), Vec2::new(width, RESOURCE_BAR_HEIGHT)),
            Transform::from_translation(bar_pos),
        ));
    }
}

fn camera_pan_zoom_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    mut camera_query: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let Ok((mut transform, mut projection)) = camera_query.single_mut() else {
        return;
    };

    let mut pan = Vec2::ZERO;
    if keys.pressed(KeyCode::ArrowLeft) || keys.pressed(KeyCode::KeyA) {
        pan.x -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD) {
        pan.x += 1.0;
    }
    if keys.pressed(KeyCode::ArrowUp) || keys.pressed(KeyCode::KeyW) {
        pan.y += 1.0;
    }
    if keys.pressed(KeyCode::ArrowDown) || keys.pressed(KeyCode::KeyS) {
        pan.y -= 1.0;
    }

    if pan != Vec2::ZERO {
        let delta = pan.normalize() * CAMERA_PAN_SPEED * time.delta_secs();
        transform.translation.x += delta.x;
        transform.translation.y += delta.y;
    }

    let mut wheel_delta = 0.0f32;
    for evt in mouse_wheel.read() {
        wheel_delta += evt.y;
    }
    if wheel_delta.abs() > f32::EPSILON {
        if let Projection::Orthographic(ref mut ortho) = *projection {
            let scale = ortho.scale * (1.0 - wheel_delta * CAMERA_ZOOM_STEP);
            ortho.scale = scale.clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
        }
    }
}

fn setup(mut commands: Commands, mut allocator: ResMut<ItemIdAllocator>) {
    commands.spawn(Camera2d);

    // Food and water never share a chunk. Cluster A: food at (0,0), water at (1,0).
    // Enough portions per cluster so 10 pawns can eat/drink and sustain 10k ticks with respawn.
    let chunk_a = ChunkId(0, 0);
    let water_a_chunk = ChunkId(1, 0);
    let id_food_a = allocator.alloc();
    commands.spawn((
        FoodReservation { portions: 12 },
        Position { chunk: chunk_a },
        ItemIdentity { item_id: id_food_a, created_at_causal_seq: 0 },
        ItemHistory::default(),
        Sprite::from_color(Color::srgb(0.9, 0.5, 0.1), Vec2::splat(SPRITE_FOOD)),
        Transform::from_translation(chunk_to_translation(&chunk_a, 0.0)),
        Name::new("Food_A"),
    ));
    let id_water_a = allocator.alloc();
    commands.spawn((
        WaterSource { portions: 12 },
        Position { chunk: water_a_chunk },
        ItemIdentity { item_id: id_water_a, created_at_causal_seq: 0 },
        ItemHistory::default(),
        Sprite::from_color(Color::srgb(0.2, 0.85, 0.95), Vec2::splat(SPRITE_WATER)),
        Transform::from_translation(chunk_to_translation(&water_a_chunk, 0.0)),
        Name::new("Water_A"),
    ));
    commands.spawn((
        RestSpot,
        Position { chunk: chunk_a },
        Sprite::from_color(Color::srgb(0.4, 0.35, 0.3), Vec2::splat(SPRITE_FOOD)),
        Transform::from_translation(chunk_to_translation(&chunk_a, 0.0)),
        Name::new("Rest_A"),
    ));
    for i in 1..=4 {
        let offset = Vec3::new((i as f32 - 2.5) * 4.0, 0.0, 1.0);
        commands.spawn((
            build_pawn_brain(),
            NeuralNetworkComponent { hunger: 0.9, thirst: 0.85, fatigue: 0.8, ..default() },
            Position { chunk: chunk_a },
            DisplayOffset(offset),
            Capabilities { can_do: vec!["Eat".into(), "Drink".into(), "Rest".into()] },
            KnownRecipes::default(),
            Sprite::from_color(Color::srgb(0.2, 0.75, 0.3), Vec2::splat(SPRITE_PAWN)),
            Transform::from_translation(chunk_to_translation(&chunk_a, 0.0) + offset),
            Name::new(format!("Pawn_A_{}", i)),
            PawnVisual,
        ));
    }

    // Cluster B near far corner to exercise large-grid propagation and long-range behavior.
    let chunk_b = ChunkId(CHUNK_EXTENT - 1, CHUNK_EXTENT - 1);
    let water_b_chunk = ChunkId(CHUNK_EXTENT - 2, CHUNK_EXTENT - 1);
    let id_food_b = allocator.alloc();
    commands.spawn((
        FoodReservation { portions: 12 },
        Position { chunk: chunk_b },
        ItemIdentity { item_id: id_food_b, created_at_causal_seq: 0 },
        ItemHistory::default(),
        Sprite::from_color(Color::srgb(0.9, 0.5, 0.1), Vec2::splat(SPRITE_FOOD)),
        Transform::from_translation(chunk_to_translation(&chunk_b, 0.0)),
        Name::new("Food_B"),
    ));
    let id_water_b = allocator.alloc();
    commands.spawn((
        WaterSource { portions: 12 },
        Position { chunk: water_b_chunk },
        ItemIdentity { item_id: id_water_b, created_at_causal_seq: 0 },
        ItemHistory::default(),
        Sprite::from_color(Color::srgb(0.2, 0.85, 0.95), Vec2::splat(SPRITE_WATER)),
        Transform::from_translation(chunk_to_translation(&water_b_chunk, 0.0)),
        Name::new("Water_B"),
    ));
    commands.spawn((
        RestSpot,
        Position { chunk: chunk_b },
        Sprite::from_color(Color::srgb(0.4, 0.35, 0.3), Vec2::splat(SPRITE_FOOD)),
        Transform::from_translation(chunk_to_translation(&chunk_b, 0.0)),
        Name::new("Rest_B"),
    ));
    for i in 1..=4 {
        let offset = Vec3::new((i as f32 - 2.5) * 4.0, 0.0, 1.0);
        commands.spawn((
            build_pawn_brain(),
            NeuralNetworkComponent { hunger: 0.9, thirst: 0.85, fatigue: 0.8, ..default() },
            Position { chunk: chunk_b },
            DisplayOffset(offset),
            Capabilities { can_do: vec!["Eat".into(), "Drink".into(), "Rest".into()] },
            KnownRecipes::default(),
            Sprite::from_color(Color::srgb(0.2, 0.75, 0.3), Vec2::splat(SPRITE_PAWN)),
            Transform::from_translation(chunk_to_translation(&chunk_b, 0.0) + offset),
            Name::new(format!("Pawn_B_{}", i)),
            PawnVisual,
        ));
    }
}

const DISCOVERY_RECIPE_FIRE: &str = "Fire";
const DISCOVERY_CURIOSITY_THRESHOLD: f32 = 4.5;
const DISCOVERY_SOCIAL_THRESHOLD: f32 = 0.7;

fn curiosity_discovery_system(
    global_clock: Res<GlobalTickClock>,
    mut quest_board: ResMut<QuestBoard>,
    mut event_queue: ResMut<CausalEventQueue>,
    mut pawns: Query<(
        Entity,
        &Name,
        &Position,
        &mut NeuralNetworkComponent,
        &mut KnownRecipes,
    )>,
    food_query: Query<&Position, With<FoodReservation>>,
    rest_query: Query<&Position, With<RestSpot>>,
    mut activity: ResMut<ActivityLog>,
    mut report: Option<ResMut<SimulationReport>>,
    mut last_tick: Local<u64>,
) {
    let started = Instant::now();
    let seq = global_clock.causal_seq();
    if seq <= *last_tick {
        perf_record("curiosity_discovery", started.elapsed());
        return;
    }
    *last_tick = seq;

    let food_chunks: std::collections::HashSet<_> = food_query.iter().map(|p| p.chunk).collect();
    let rest_chunks: std::collections::HashSet<_> = rest_query.iter().map(|p| p.chunk).collect();

    let mut snapshot: Vec<(Entity, String, ChunkId, f32, bool)> = Vec::new();

    for (entity, name, position, mut nn, mut known) in pawns.iter_mut() {
        nn.curiosity += 0.001;
        let curiosity_now = nn.curiosity;
        if nn.curiosity > 5.0 {
            nn.curiosity = 0.0;
            quest_board.active_quests.push(Quest {
                need: "Learn Fire".to_string(),
                requester: entity,
                chunk: position.chunk,
                provider: None,
                status: QuestStatus::Open,
            });
        }

        let has_fire = known.recipes.contains(DISCOVERY_RECIPE_FIRE);
        let can_discover_here = food_chunks.contains(&position.chunk) && rest_chunks.contains(&position.chunk);
        if !has_fire && can_discover_here && curiosity_now >= DISCOVERY_CURIOSITY_THRESHOLD {
            known.recipes.insert(DISCOVERY_RECIPE_FIRE.to_string());
            activity.push(format!("{} discovered {}", name, DISCOVERY_RECIPE_FIRE));
            if let Some(ref mut report) = report {
                report.bump("recipe_discoveries");
            }
        }

        snapshot.push((
            entity,
            name.to_string(),
            position.chunk,
            nn.social,
            known.recipes.contains(DISCOVERY_RECIPE_FIRE),
        ));
    }

    let mut taught_this_tick = std::collections::HashSet::new();
    for (teacher_entity, teacher_name, teacher_chunk, teacher_social, teacher_has_fire) in &snapshot {
        if !*teacher_has_fire || *teacher_social < DISCOVERY_SOCIAL_THRESHOLD {
            continue;
        }
        for (learner_entity, learner_name, learner_chunk, _learner_social, learner_has_fire) in &snapshot {
            if *learner_entity == *teacher_entity || *learner_has_fire {
                continue;
            }
            if !taught_this_tick.insert(*learner_entity) {
                continue;
            }
            let dist = chebyshev_distance(teacher_chunk, learner_chunk);
            if dist > 1 {
                continue;
            }
            let deliver_at = seq + propagation_delay(teacher_chunk, learner_chunk, heartbeat::C);
            event_queue.push_at(
                CausalEventKind::DiscoveryPropagated {
                    recipe: DISCOVERY_RECIPE_FIRE.to_string(),
                    from: *teacher_entity,
                    to: *learner_entity,
                },
                *teacher_chunk,
                deliver_at,
            );
            activity.push(format!(
                "{} teaching {} to {}",
                teacher_name, DISCOVERY_RECIPE_FIRE, learner_name
            ));
            if let Some(ref mut report) = report {
                report.bump("recipe_teaching_events");
            }
        }
    }
    perf_record("curiosity_discovery", started.elapsed());
}

/// Chunk grid extent (0..=CHUNK_EXTENT). Used for respawn bounds.
const CHUNK_EXTENT: u32 = WORLD_CHUNK_EXTENT;

/// Target counts for respawn: maintain at least this many food and water entities in the world.
const TARGET_FOOD_COUNT: usize = 6;
const TARGET_WATER_COUNT: usize = 4;
const SIMLIFE_GRASS_MAX: u32 = 10;
const SIMLIFE_BASE_FOOD_PORTIONS: u32 = 8;
const SIMLIFE_GRASS_TO_FOOD_DIVISOR: u32 = 2;
const SIMLIFE_MAX_FOOD_PORTIONS: u32 = 12;

struct HypergraphRegionalOutputWrite;
struct ChemistryNoiseFloorWrite;
struct SimLifeGrayScottWrite;

// Gray-Scott reaction-diffusion constants (Tier 5/6 biomass field).
// Spot-forming parameter regime: F=0.055, k=0.062.
const GS_DU: f32 = 0.16;
const GS_DV: f32 = 0.08;
const GS_F_BASE: f32 = 0.055;
const GS_K: f32 = 0.062;
const GS_DT: f32 = 1.0;
/// GS runs every this many sim ticks; at target 1000 Hz → 200 Hz GS rate.
const GS_UPDATE_INTERVAL_TICKS: u64 = 5;
/// Cells with V above this are considered active; their neighbors are queued as frontier.
const GS_V_ACTIVE_THRESHOLD: f32 = 0.01;
/// Hypergraph clustering scales the local GS feed rate.
const GS_F_CLUSTERING_SCALE: f32 = 0.015;
/// Hypergraph causal volume also modulates GS feed.
const GS_F_CAUSAL_VOLUME_SCALE: f32 = 0.012;
/// Directive retune: soften clustering influence on local growth.
const GS_CLUSTERING_MULTIPLIER: f32 = 0.5;
/// Directive retune: soften causal-volume influence on local growth.
const GS_CAUSAL_VOLUME_MULTIPLIER: f32 = 0.6;

#[derive(Resource, Debug, Clone)]
struct ChemistryState {
    receptor_noise_floor_by_chunk: HashMap<ChunkId, f32>,
}

impl Default for ChemistryState {
    fn default() -> Self {
        Self {
            receptor_noise_floor_by_chunk: HashMap::new(),
        }
    }
}

#[cfg(debug_assertions)]
#[derive(Resource, Debug, Clone, Copy)]
struct HypergraphDebugViz {
    enabled: bool,
}

#[cfg(debug_assertions)]
impl Default for HypergraphDebugViz {
    fn default() -> Self {
        Self { enabled: false }
    }
}

/// Tracks last sim tick we ran respawn. Ensures we run respawn once per tick.
#[derive(Resource, Default)]
struct RespawnState {
    last_tick: u64,
}

/// Gray-Scott Tier 5/6 SimLife field.
/// U = substrate (implicit default 1.0). V = biomass/organism (implicit default 0.0).
/// `grass_per_chunk` is derived from V after each GS step and read by surface resource systems.
#[derive(Resource, Debug, Clone)]
struct SimLifeState {
    last_tick: u64,
    last_gs_tick: u64,
    /// Substrate concentration per active chunk; absent entry = 1.0.
    u_field: HashMap<ChunkId, f32>,
    /// Biomass concentration per active chunk; absent entry = 0.0.
    v_field: HashMap<ChunkId, f32>,
    /// Sparse frontier: cells to process next GS step.
    /// Contains cells with V > threshold AND their 4-connected neighbors.
    gs_active: HashSet<ChunkId>,
    /// Derived grass pressure: grass = (V * SIMLIFE_GRASS_MAX) as u32.
    /// read by resource_respawn + food_portions.
    grass_per_chunk: HashMap<ChunkId, u32>,
}

impl Default for SimLifeState {
    fn default() -> Self {
        Self {
            last_tick: 0,
            last_gs_tick: 0,
            u_field: HashMap::new(),
            v_field: HashMap::new(),
            gs_active: HashSet::new(),
            grass_per_chunk: HashMap::new(),
        }
    }
}

#[cfg(test)]
fn advance_simlife_grass(current_seq: u64, simlife: &mut SimLifeState) {
    let mut charter = SpatialCharter::default();
    advance_simlife_grass_with_hypergraph(current_seq, simlife, None, &mut charter);
}

/// Advance the Gray-Scott Tier 5/6 field by one logical tick.
/// Seeding happens automatically on first call. GS update runs every GS_UPDATE_INTERVAL_TICKS.
fn advance_simlife_grass_with_hypergraph(
    current_seq: u64,
    simlife: &mut SimLifeState,
    hypergraph: Option<&HypergraphSubstrate>,
    charter: &mut SpatialCharter,
) {
    if current_seq <= simlife.last_tick {
        return;
    }
    simlife.last_tick = current_seq;

    // Seed initial state when all active cells have been exhausted (or on cold start).
    if simlife.gs_active.is_empty() {
        gs_seed_initial_state(simlife);
    }

    // Throttle: only run the GS stencil every GS_UPDATE_INTERVAL_TICKS sim ticks.
    if current_seq.saturating_sub(simlife.last_gs_tick) < GS_UPDATE_INTERVAL_TICKS {
        return;
    }
    simlife.last_gs_tick = current_seq;

    gs_update(current_seq, simlife, hypergraph, charter);
}

/// Set up four widely-spaced seed clusters (<0.01 % of the 256×256 grid).
/// Initial conditions: u=0.50, v=0.25 — classical GS spot-seed amplitude.
fn gs_seed_initial_state(simlife: &mut SimLifeState) {
    let seeds: &[(u32, u32)] = &[(64, 64), (64, 192), (192, 64), (192, 192)];
    for &(x, y) in seeds {
        let chunk = ChunkId(x, y);
        simlife.u_field.insert(chunk, 0.5);
        simlife.v_field.insert(chunk, 0.25);
        simlife.gs_active.insert(chunk);
        for (nx, ny) in gs_neighbor_coords(x, y) {
            simlife.gs_active.insert(ChunkId(nx, ny));
        }
    }
}

/// 4-neighbor coordinates of (cx, cy) with Neumann (zero-flux) boundary clamping.
fn gs_neighbor_coords(cx: u32, cy: u32) -> Vec<(u32, u32)> {
    let mut out = Vec::with_capacity(4);
    if cx > 0             { out.push((cx - 1, cy)); }
    if cx < CHUNK_EXTENT  { out.push((cx + 1, cy)); }
    if cy > 0             { out.push((cx, cy - 1)); }
    if cy < CHUNK_EXTENT  { out.push((cx, cy + 1)); }
    out
}

/// Discrete 4-neighbor Laplacian with Neumann BC: boundary directions use the center value
/// so the derivative normal to the boundary is zero.
fn gs_laplacian(cx: u32, cy: u32, field: &HashMap<ChunkId, f32>, default: f32) -> f32 {
    let center = *field.get(&ChunkId(cx, cy)).unwrap_or(&default); // GS_SPARSE_FIELD_DEFAULT
    let left  = if cx > 0            { *field.get(&ChunkId(cx - 1, cy)).unwrap_or(&default) } else { center }; // GS_SPARSE_FIELD_DEFAULT
    let right = if cx < CHUNK_EXTENT { *field.get(&ChunkId(cx + 1, cy)).unwrap_or(&default) } else { center }; // GS_SPARSE_FIELD_DEFAULT
    let down  = if cy > 0            { *field.get(&ChunkId(cx, cy - 1)).unwrap_or(&default) } else { center }; // GS_SPARSE_FIELD_DEFAULT
    let up    = if cy < CHUNK_EXTENT { *field.get(&ChunkId(cx, cy + 1)).unwrap_or(&default) } else { center }; // GS_SPARSE_FIELD_DEFAULT
    left + right + down + up - 4.0 * center
}

/// One Gray-Scott update step over the sparse active frontier.
/// Read phase computes all delta-U/V from the CURRENT field (no in-place hazard).
/// Write phase applies updates via charter leases; denied cells preserve their old value.
fn gs_update(
    current_seq: u64,
    simlife: &mut SimLifeState,
    hypergraph: Option<&HypergraphSubstrate>,
    charter: &mut SpatialCharter,
) {
    let active_cells: Vec<ChunkId> = simlife.gs_active.iter().copied().collect();

    // ── READ PHASE ─────────────────────────────────────────────────────────
    // Compute new (u, v) for every active cell from the *current* (unmodified) field.
    let mut updates: Vec<(ChunkId, f32, f32)> = Vec::with_capacity(active_cells.len());
    for &cell in &active_cells {
        let ChunkId(cx, cy) = cell;
        let u = *simlife.u_field.get(&cell).unwrap_or(&1.0); // GS_SPARSE_FIELD_DEFAULT
        let v = *simlife.v_field.get(&cell).unwrap_or(&0.0); // GS_SPARSE_FIELD_DEFAULT

        // Local feed rate: base + softened hypergraph clustering/causal-volume modulation.
        let f_local = if let Some(hg) = hypergraph {
            if let Some(output) = hg.output_for_chunk(cx, cy) {
                let clustering = output.clustering.clamp(0.0, 1.0)
                    * GS_F_CLUSTERING_SCALE
                    * GS_CLUSTERING_MULTIPLIER;
                let causal_volume = output.causal_volume.clamp(0.0, 1.0)
                    * GS_F_CAUSAL_VOLUME_SCALE
                    * GS_CAUSAL_VOLUME_MULTIPLIER;
                (GS_F_BASE + clustering + causal_volume).clamp(0.01, 0.10)
            } else {
                GS_F_BASE
            }
        } else {
            GS_F_BASE
        };

        let lap_u = gs_laplacian(cx, cy, &simlife.u_field, 1.0);
        let lap_v = gs_laplacian(cx, cy, &simlife.v_field, 0.0);

        let uvv = u * v * v;
        let new_u = (u + GS_DT * (GS_DU * lap_u - uvv + f_local * (1.0 - u))).clamp(0.0, 1.0);
        let new_v = (v + GS_DT * (GS_DV * lap_v + uvv - (f_local + GS_K) * v)).clamp(0.0, 1.0);

        updates.push((cell, new_u, new_v));
    }

    // ── WRITE PHASE ────────────────────────────────────────────────────────
    // Apply updates through charter leases. Build next active frontier.
    let mut new_active: HashSet<ChunkId> = HashSet::new();
    let mut lease_handles: Vec<LeaseHandle> = Vec::new();

    for (cell, new_u, new_v) in updates {
        let ChunkId(cx, cy) = cell;
        let lease_req = SpatialLease {
            primary: cell,
            fringe: vec![],
            intent: LeaseIntent {
                reads: vec![],
                writes: vec![TypeId::of::<SimLifeGrayScottWrite>()],
            },
            granted_at_causal_seq: current_seq,
        };
        match charter.request_lease(lease_req, current_seq) {
            Ok(handle) => {
                simlife.u_field.insert(cell, new_u);
                simlife.v_field.insert(cell, new_v);
                // Derive grass from V concentration.
                let grass = (new_v * SIMLIFE_GRASS_MAX as f32) as u32;
                if grass > 0 {
                    simlife.grass_per_chunk.insert(cell, grass);
                } else {
                    simlife.grass_per_chunk.remove(&cell);
                }
                // Maintain sparse frontier.
                if new_v > GS_V_ACTIVE_THRESHOLD {
                    new_active.insert(cell);
                    for (nx, ny) in gs_neighbor_coords(cx, cy) {
                        new_active.insert(ChunkId(nx, ny));
                    }
                }
                lease_handles.push(handle);
            }
            Err(_) => {
                // Denied: preserve old value; keep cell in active set for retry next step.
                let old_v = *simlife.v_field.get(&cell).unwrap_or(&0.0); // GS_SPARSE_FIELD_DEFAULT
                if old_v > GS_V_ACTIVE_THRESHOLD {
                    new_active.insert(cell);
                    for (nx, ny) in gs_neighbor_coords(cx, cy) {
                        new_active.insert(ChunkId(nx, ny));
                    }
                }
            }
        }
    }

    simlife.gs_active = new_active;

    for handle in lease_handles {
        charter.release_lease(handle);
    }
}

fn food_portions_from_grass(grass: u32) -> u32 {
    (SIMLIFE_BASE_FOOD_PORTIONS + grass / SIMLIFE_GRASS_TO_FOOD_DIVISOR)
        .min(SIMLIFE_MAX_FOOD_PORTIONS)
}

fn preferred_food_chunks() -> [ChunkId; 2] {
    [
        ChunkId(0, 0),
        ChunkId(CHUNK_EXTENT - 1, CHUNK_EXTENT - 1),
    ]
}

fn preferred_water_chunks() -> [ChunkId; 2] {
    [
        ChunkId(1, 0),
        ChunkId(CHUNK_EXTENT - 2, CHUNK_EXTENT - 1),
    ]
}

fn simlife_tick_system(
    global_clock: Res<GlobalTickClock>,
    mut simlife: ResMut<SimLifeState>,
    hypergraph: Res<HypergraphSubstrate>,
    mut charter: ResMut<SpatialCharter>,
) {
    let started = Instant::now();
    // Causal ordering: run after sim_tick_driver, then respawn reads same-tick SimLife state.
    if tier10_enabled_from_args() {
        advance_simlife_grass_with_hypergraph(
            global_clock.causal_seq(),
            &mut simlife,
            Some(&hypergraph),
            &mut charter,
        );
    } else {
        advance_simlife_grass_with_hypergraph(
            global_clock.causal_seq(),
            &mut simlife,
            None,
            &mut charter,
        );
    }
    perf_record("simlife_tick", started.elapsed());
}

fn hypergraph_tick_system(
    global_clock: Res<GlobalTickClock>,
    mut charter: ResMut<SpatialCharter>,
    mut substrate: ResMut<HypergraphSubstrate>,
    mut chemistry: ResMut<ChemistryState>,
    mut report: Option<ResMut<SimulationReport>>,
) {
    let started = Instant::now();
    if !tier10_enabled_from_args() {
        perf_record("hypergraph_tick", started.elapsed());
        return;
    }

    let seq = global_clock.causal_seq();
    let patch_chunk_size = substrate.config().patch_chunk_size;
    let mut active_handles: Vec<LeaseHandle> = Vec::new();

    let step_started = Instant::now();
    let stats = substrate.step_with_permissions(seq, |coord| {
        let chunk_x = coord.x.saturating_mul(patch_chunk_size).min(CHUNK_EXTENT);
        let chunk_y = coord.y.saturating_mul(patch_chunk_size).min(CHUNK_EXTENT);
        let request = SpatialLease {
            primary: ChunkId(chunk_x, chunk_y),
            fringe: vec![],
            intent: LeaseIntent {
                reads: vec![],
                writes: vec![
                    TypeId::of::<HypergraphRegionalOutputWrite>(),
                    TypeId::of::<ChemistryNoiseFloorWrite>(),
                ],
            },
            granted_at_causal_seq: seq,
        };

        let lease_started = Instant::now();
        match charter.request_lease(request, seq) {
            Ok(handle) => {
                perf_record("charter_lease_acquire_grant", lease_started.elapsed());
                active_handles.push(handle);
                true
            }
            Err(_) => {
                perf_record("charter_lease_acquire_deny", lease_started.elapsed());
                false
            }
        }
    });
    perf_record("hypergraph_step_with_neighbors", step_started.elapsed());

    let chemistry_started = Instant::now();
    for coord in substrate.patch_coords() {
        let (chunk_x, chunk_y) = substrate.patch_primary_chunk(coord);
        if let Some(output) = substrate.patch_output(coord) {
            let noise_floor = (output.clustering.clamp(0.0, 1.0) * HYPERGRAPH_NOISE_FLOOR_MULTIPLIER)
                .clamp(0.0, HYPERGRAPH_NOISE_FLOOR_MAX);
            chemistry
                .receptor_noise_floor_by_chunk
                .insert(ChunkId(chunk_x, chunk_y), noise_floor);
        }
    }
    perf_record("chemistry_noise_floor_hook", chemistry_started.elapsed());

    let release_started = Instant::now();
    for handle in active_handles {
        charter.release_lease(handle);
    }
    perf_record("charter_lease_release", release_started.elapsed());

    if let Some(ref mut report) = report {
        if stats.considered > 0 {
            report.bump("hypergraph_ticks");
        }
        for _ in 0..stats.rewritten {
            report.bump("hypergraph_rewrites");
        }
        for _ in 0..stats.denied {
            report.bump("hypergraph_lease_denials");
        }
    }
    perf_record("hypergraph_tick", started.elapsed());
}

#[cfg(debug_assertions)]
fn hypergraph_debug_input_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut viz: ResMut<HypergraphDebugViz>,
    mut substrate: ResMut<HypergraphSubstrate>,
) {
    if keys.just_pressed(KeyCode::KeyV) {
        viz.enabled = !viz.enabled;
    }

    let mut chaos = substrate.chaos();
    if keys.just_pressed(KeyCode::KeyJ) {
        chaos = (chaos - HYPERGRAPH_DEBUG_CHAOS_STEP).max(0.0);
    }
    if keys.just_pressed(KeyCode::KeyK) {
        chaos = (chaos + HYPERGRAPH_DEBUG_CHAOS_STEP).min(1.0);
    }
    substrate.set_chaos(chaos);
}

#[cfg(not(debug_assertions))]
fn hypergraph_debug_input_system() {}

#[cfg(debug_assertions)]
fn hypergraph_debug_viz_system(
    viz: Res<HypergraphDebugViz>,
    substrate: Res<HypergraphSubstrate>,
    mut gizmos: Gizmos,
) {
    if !viz.enabled {
        return;
    }

    for coord in substrate.patch_coords() {
        let Some(output) = substrate.patch_output(coord) else {
            continue;
        };
        let (chunk_x, chunk_y) = substrate.patch_primary_chunk(coord);
        let center_x = chunk_x as f32 * CHUNK_PIXEL + CHUNK_PIXEL * 0.5;
        let center_y = chunk_y as f32 * CHUNK_PIXEL + CHUNK_PIXEL * 0.5;

        let node_radius = 2.0 + output.density * 5.0;
        let color = Color::srgba(
            (0.2 + output.clustering * 0.8).clamp(0.0, 1.0),
            (0.2 + output.causal_volume * 0.8).clamp(0.0, 1.0),
            (0.2 + output.avg_arity * 0.8).clamp(0.0, 1.0),
            0.7,
        );
        gizmos.circle_2d(Vec2::new(center_x, center_y), node_radius, color);

        let arm = 4.0 + output.clustering * 10.0;
        gizmos.line_2d(
            Vec2::new(center_x - arm, center_y),
            Vec2::new(center_x + arm, center_y),
            color,
        );
        gizmos.line_2d(
            Vec2::new(center_x, center_y - arm),
            Vec2::new(center_x, center_y + arm),
            color,
        );
    }
}

#[cfg(not(debug_assertions))]
fn hypergraph_debug_viz_system() {}

/// Deterministically pick an empty chunk without materializing a full-grid empty list.
fn select_empty_chunk(seed: u64, occupied: &HashSet<ChunkId>) -> Option<ChunkId> {
    let side = CHUNK_EXTENT as u64 + 1;
    let total = side * side;
    if occupied.len() as u64 >= total {
        return None;
    }

    let start = seed % total;
    for offset in 0..total {
        let idx = (start + offset) % total;
        let x = (idx / side) as u32;
        let y = (idx % side) as u32;
        let chunk = ChunkId(x, y);
        if !occupied.contains(&chunk) {
            return Some(chunk);
        }
    }

    None
}

/// Spawns food and water at deterministic empty chunks when below target. Food and water never share a chunk.
/// Run after sim_tick_driver.
fn resource_respawn_system(
    global_clock: Res<GlobalTickClock>,
    mut state: ResMut<RespawnState>,
    mut commands: Commands,
    mut allocator: ResMut<ItemIdAllocator>,
    simlife: Res<SimLifeState>,
    food_query: Query<&Position, With<FoodReservation>>,
    water_query: Query<&Position, With<WaterSource>>,
) {
    let started = Instant::now();
    let current = global_clock.causal_seq();
    if current <= state.last_tick {
        perf_record("resource_respawn", started.elapsed());
        return;
    }
    state.last_tick = current;

    let food_chunks: HashSet<_> = food_query.iter().map(|p| p.chunk).collect();
    let water_chunks: HashSet<_> = water_query.iter().map(|p| p.chunk).collect();
    let occupied: HashSet<_> = food_chunks.union(&water_chunks).copied().collect();

    let need_food = food_chunks.len() < TARGET_FOOD_COUNT;
    let need_water = water_chunks.len() < TARGET_WATER_COUNT;

    let food_chunk = if need_food {
        preferred_food_chunks()
            .into_iter()
            .find(|chunk| !occupied.contains(chunk))
            .or_else(|| select_empty_chunk(current, &occupied))
    } else {
        None
    };

    if let Some(chunk) = food_chunk {
        let id = allocator.alloc();
        let grass = *simlife.grass_per_chunk.get(&chunk).unwrap_or(&0);
        let portions = food_portions_from_grass(grass);
        commands.spawn((
            FoodReservation { portions },
            Position { chunk },
            ItemIdentity { item_id: id, created_at_causal_seq: current },
            ItemHistory::default(),
            Sprite::from_color(Color::srgb(0.9, 0.5, 0.1), Vec2::splat(SPRITE_FOOD)),
            Transform::from_translation(chunk_to_translation(&chunk, 0.0)),
            Name::new("Food_respawn"),
        ));
    }

    if need_water {
        let occupied_after: HashSet<_> = food_chunks
            .iter()
            .copied()
            .chain(water_chunks.iter().copied())
            .chain(food_chunk)
            .collect();
        let water_chunk = preferred_water_chunks()
            .into_iter()
            .find(|chunk| !occupied_after.contains(chunk))
            .or_else(|| select_empty_chunk(current.wrapping_add(1), &occupied_after));
        if let Some(chunk) = water_chunk {
            let id = allocator.alloc();
            commands.spawn((
                WaterSource { portions: 12 },
                Position { chunk },
                ItemIdentity { item_id: id, created_at_causal_seq: current },
                ItemHistory::default(),
                Sprite::from_color(Color::srgb(0.2, 0.85, 0.95), Vec2::splat(SPRITE_WATER)),
                Transform::from_translation(chunk_to_translation(&chunk, 0.0)),
                Name::new("Water_respawn"),
            ));
        }
    }
    perf_record("resource_respawn", started.elapsed());
}

fn sim_tick_driver(
    time: Res<Time>,
    mut global_clock: ResMut<GlobalTickClock>,
    mut accumulator: ResMut<SimTickAccumulator>,
    scale: Res<SimTimeScale>,
    mut event_queue: ResMut<CausalEventQueue>,
    mut quest_board: ResMut<QuestBoard>,
    mut activity: ResMut<ActivityLog>,
    mut evaluation_state: ResMut<ai::DispatcherEvaluationState>,
    mut pawn_query: Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
    capabilities_query: Query<&Capabilities>,
    mut known_recipes_query: Query<&mut KnownRecipes>,
    food_query: Query<(&Position, &FoodReservation)>,
    water_query: Query<(&Position, &WaterSource)>,
    hypergraph: Res<HypergraphSubstrate>,
    mut report: Option<ResMut<SimulationReport>>,
    log_settings: Option<Res<SimulationLogSettings>>,
) {
    accumulator.0 += time.delta_secs() * scale.0 * SIM_TICKS_PER_SECOND_AT_1X;
    let stdout_enabled = log_settings
        .as_deref()
        .map(|settings| settings.stdout_enabled)
        .unwrap_or(true);
    while accumulator.0 >= 1.0 {
        advance_simulation_one_tick(
            &mut global_clock,
            &mut event_queue,
            &mut quest_board,
            &mut activity,
            &mut evaluation_state,
            &mut pawn_query,
            &capabilities_query,
            &mut known_recipes_query,
            &food_query,
            &water_query,
            &hypergraph,
            report.as_deref_mut(),
            stdout_enabled,
        );
        accumulator.0 -= 1.0;
    }
}

/// Min/max scale for display sanity; effectively unbounded for gameplay.
const MIN_SCALE: f32 = 0.01;
const MAX_SCALE: f32 = 1000.0;

fn time_scale_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut scale: ResMut<SimTimeScale>,
) {
    if keys.just_pressed(KeyCode::KeyR) {
        scale.0 = 1.0;
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        scale.0 = (scale.0 * 1.5).min(MAX_SCALE);
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        scale.0 = (scale.0 / 1.5).max(MIN_SCALE);
    }
    if keys.just_pressed(KeyCode::KeyP) {
        scale.0 = if scale.0 == 0.0 { 1.0 } else { 0.0 };
    }
}
