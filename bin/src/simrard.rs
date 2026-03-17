use bevy::prelude::*;
use bevy::ecs::message::{MessageReader, Messages};
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::MouseWheel;
use bevy::sprite::Sprite;
use bevy::text::{Font, FontSmoothing};
use std::cmp::Ordering;
use std::any::TypeId;
use std::collections::{HashMap, HashSet, VecDeque};
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
use simrard_lib_tier4::{advance_tier4, Tier4State};
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

// Temporary binary-isolation toggles for substrate performance analysis.
const ISOLATION_DISABLE_T60_AND_EXTRA_COUNTERS: bool = false;
const ISOLATION_REVERT_HYPERGRAPH_CADENCE: bool = false;
const ISOLATION_MINIMAL_SEEDING: bool = false;
const ISOLATION_DISABLE_PER_SECOND_PROBES: bool = false;

static HEADLESS_PERF: OnceLock<Mutex<PerfAudit>> = OnceLock::new();
static TIER10_ENABLED: OnceLock<bool> = OnceLock::new();
static HEADLESS_SUBSTRATE: OnceLock<bool> = OnceLock::new();
static BENCHMARK_SECONDS: OnceLock<f64> = OnceLock::new();
static INTERACTIVE_WITH_TIER1: OnceLock<bool> = OnceLock::new();

#[derive(Resource, Debug, Clone, Copy)]
struct VisualDebug {
    enabled: bool,
}

#[derive(Resource, Debug, Clone, Copy)]
struct SimulationModeRuntime {
    mode: SimulationMode,
    interactive_with_tier1: bool,
}

#[derive(Resource, Clone)]
struct HudFontHandle(Handle<Font>);

const LIVE_STATS_FONT_SIZE: f32 = 11.0;

fn live_stats_overlay_system(
    mut commands: Commands,
    visual: Res<VisualDebug>,
    hud_font: Res<HudFontHandle>,
    tier4: Res<Tier4State>,
    simlife: Res<SimLifeState>,
    hypergraph_stats: Res<HypergraphRuntimeStats>,
    thermal: Res<ThermalState>,
    global_clock: Res<GlobalTickClock>,
    time: Res<Time>,
    camera_query: Query<(&Transform, &Projection), With<Camera2d>>,
    mut panel_queries: ParamSet<(
        Query<(&mut Text, &mut TextFont, &mut Visibility), With<LiveStatsPanelTextUi>>,
        Query<&mut Visibility, With<LiveStatsPanelUiRoot>>,
    )>,
    mut local: Local<LiveStatsPanelLocal>,
    detail: LiveStatsDetailParams,
) {
    let (cam_x, cam_y, area_max_x, area_max_y) = match camera_query.single() {
        Ok((transform, Projection::Orthographic(ortho))) => (
            transform.translation.x,
            transform.translation.y,
            ortho.area.max.x,
            ortho.area.max.y,
        ),
        _ => return,
    };

    // Keep layout function in the live path for resolution behavior parity checks.
    let _ = live_stats_panel_layout(cam_x, cam_y, area_max_x, area_max_y);

    let now = time.elapsed_secs();
    let dt = time.delta_secs().max(0.0001);
    local.fps = 1.0 / dt;

    if local.prev_time > 0.0 {
        let tick = global_clock.causal_seq();
        let tick_dt = (now - local.prev_time).max(0.0001);
        local.ticks_per_sec = (tick.saturating_sub(local.prev_tick) as f32) / tick_dt;
        local.prev_tick = tick;
    } else {
        local.prev_tick = global_clock.causal_seq();
    }
    local.prev_time = now;

    let current_delta_k = (thermal.latest_peak_temperature_k - thermal.sink_temperature_k).max(0.0);
    local.thermal_delta_samples.push_back((now, current_delta_k));
    while let Some((t, _)) = local.thermal_delta_samples.front() {
        if now - *t > 5.0 {
            local.thermal_delta_samples.pop_front();
        } else {
            break;
        }
    }
    let thermal_avg_5s = if local.thermal_delta_samples.is_empty() {
        0.0
    } else {
        let sum: f32 = local.thermal_delta_samples.iter().map(|(_, v)| *v).sum();
        sum / local.thermal_delta_samples.len() as f32
    };

    // Track rewrite rate using cumulative rewritten count over rolling 5s window.
    local
        .hyper_rewrite_samples
        .push_back((now, hypergraph_stats.rewritten_total));
    while let Some((t, _)) = local.hyper_rewrite_samples.front() {
        if now - *t > 5.0 {
            local.hyper_rewrite_samples.pop_front();
        } else {
            break;
        }
    }
    let rewrite_rate = if local.hyper_rewrite_samples.len() >= 2 {
        let (first_t, first_v) = match local.hyper_rewrite_samples.front() {
            Some(value) => *value,
            None => (now, 0),
        };
        let (last_t, last_v) = match local.hyper_rewrite_samples.back() {
            Some(value) => *value,
            None => (now, 0),
        };
        let dt = (last_t - first_t).max(0.0001);
        (last_v.saturating_sub(first_v) as f32) / dt
    } else {
        0.0
    };

    // Record births/deaths from new Tier4 metrics and compute 5s rolling sums.
    if local.prev_tier_metric_len > tier4.metrics.len() {
        local.prev_tier_metric_len = 0;
    }
    for metrics in tier4.metrics.iter().skip(local.prev_tier_metric_len) {
        local
            .tier4_birth_death_samples
            .push_back((now, metrics.repro_grants, metrics.deaths));
    }
    local.prev_tier_metric_len = tier4.metrics.len();
    while let Some((t, _, _)) = local.tier4_birth_death_samples.front() {
        if now - *t > 5.0 {
            local.tier4_birth_death_samples.pop_front();
        } else {
            break;
        }
    }
    let births_5s: u64 = local.tier4_birth_death_samples.iter().map(|(_, b, _)| *b).sum();
    let deaths_5s: u64 = local.tier4_birth_death_samples.iter().map(|(_, _, d)| *d).sum();

    let gs_active = simlife.gs_active.len();
    let total_cells = ((CHUNK_EXTENT as u64) + 1).pow(2) as f32;
    let gs_coverage_pct = if total_cells > 0.0 {
        (gs_active as f32 / total_cells) * 100.0
    } else {
        0.0
    };

    let stats_text = format!(
        "Live Stats\nFPS / ticks-sec: {:.1} / {:.1}\nThermal sink Δ: +{:.2}K (5s avg +{:.2}K)\nGS active coverage: {:.3}%\nHypergraph rewrite rate: {:.2}/s\nTier-4 births/deaths (5s): {}/{}",
        local.fps,
        local.ticks_per_sec,
        current_delta_k,
        thermal_avg_5s,
        gs_coverage_pct,
        rewrite_rate,
        births_5s,
        deaths_5s,
    );

    let seq = global_clock.causal_seq();
    let pause = if detail.scale.0 == 0.0 { " [PAUSED]" } else { "" };
    #[cfg(debug_assertions)]
    let hypergraph_controls = if detail.hypergraph_viz.enabled {
        "J/K chaos  H hyper-viz:on"
    } else {
        "J/K chaos  H hyper-viz:off"
    };
    #[cfg(not(debug_assertions))]
    let hypergraph_controls = "";

    let sim_status = format!(
        "\n\nSim tick: {}  Speed: {:.2}x{}\nKeys: R reset  [ ] speed  P pause  V visual  Arrows/WASD pan  Wheel zoom\nHypergraph chaos: {:.2} {}\nVisual Debug: {}",
        seq,
        detail.scale.0,
        pause,
        detail.hypergraph.chaos(),
        hypergraph_controls,
        if visual.enabled { "ON" } else { "OFF" }
    );

    let food_count = detail.food_query.iter().count();
    let food_info = detail
        .food_query
        .iter()
        .map(|(pos, f)| format!("{:?}({})", pos.chunk, f.portions))
        .collect::<Vec<_>>()
        .join(", ");
    let water_count = detail.water_query.iter().count();
    let water_info = detail
        .water_query
        .iter()
        .map(|(pos, w)| format!("{:?}({})", pos.chunk, w.portions))
        .collect::<Vec<_>>()
        .join(", ");
    let resource_lines = format!(
        "\n\nResources:\n  Food {}: {}\n  Water {}: {}",
        food_count, food_info, water_count, water_info,
    );

    let quest_lines = if detail.quest_board.active_quests.is_empty() {
        "\n\nQuests: (none)".to_string()
    } else {
        std::iter::once("\n\nQuests:".to_string())
            .chain(detail.quest_board.active_quests.iter().take(10).map(|q| {
                let status = match q.status {
                    QuestStatus::Open => "Open".to_string(),
                    QuestStatus::Completed => "Completed".to_string(),
                    QuestStatus::InProgress { provider } => {
                        let provider_name = detail
                            .pawn_names
                            .get(provider)
                            .map(|n| n.to_string())
                            .unwrap_or_else(|_| format!("{:?}", provider));
                        format!("InProgress({})", provider_name)
                    }
                };
                format!("  {} @ {:?} - {}", q.need, q.chunk, status)
            }))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let activity_lines = if detail.activity.0.is_empty() {
        "\n\nActivity: (none yet)".to_string()
    } else {
        std::iter::once("\n\nActivity:".to_string())
            .chain(detail.activity.0.iter().rev().take(8).cloned())
            .collect::<Vec<_>>()
            .join("\n")
    };

    let last_rewrites = detail.audit_log.last_n(5);
    let audit_lines = if last_rewrites.is_empty() {
        "\n\nAudit: no rewrites recorded".to_string()
    } else {
        std::iter::once("\n\nLast 5 rewrites:".to_string())
            .chain(last_rewrites.iter().map(|entry| {
                format!("  @{} - {} rewrites @ {:.1}s", entry.tick, entry.rewrite_count, entry.elapsed_secs)
            }))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let stats_text = format!(
        "{}{}{}{}{}{}",
        stats_text, sim_status, resource_lines, quest_lines, activity_lines, audit_lines
    );

    if !visual.enabled {
        if let Ok((_, _, mut vis)) = panel_queries.p0().single_mut() {
            *vis = Visibility::Hidden;
        }
        if let Ok(mut vis) = panel_queries.p1().single_mut() {
            *vis = Visibility::Hidden;
        }
        return;
    }

    if let Ok((mut text, mut text_font, mut vis)) = panel_queries.p0().single_mut() {
        text.0 = stats_text;
        text_font.font = hud_font.0.clone();
        text_font.font_size = LIVE_STATS_FONT_SIZE;
        text_font.font_smoothing = FontSmoothing::AntiAliased;
        *vis = Visibility::Visible;
        if let Ok(mut panel_vis) = panel_queries.p1().single_mut() {
            *panel_vis = Visibility::Visible;
        }
    } else {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(24.0),
                top: Val::Px(24.0),
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.10, 0.14, 0.62)),
            LiveStatsPanelUiRoot,
            Visibility::Visible,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(stats_text),
                TextFont {
                    font: hud_font.0.clone(),
                    font_size: LIVE_STATS_FONT_SIZE,
                    font_smoothing: FontSmoothing::AntiAliased,
                    ..default()
                },
                TextColor(Color::srgba(0.92, 0.94, 0.98, 0.94)),
                LiveStatsPanelTextUi,
                Visibility::Visible,
            ));
        });
    }
}



#[derive(SystemParam)]
struct LiveStatsDetailParams<'w, 's> {
    scale: Res<'w, SimTimeScale>,
    hypergraph: Res<'w, HypergraphSubstrate>,
    #[cfg(debug_assertions)]
    hypergraph_viz: Res<'w, HypergraphDebugViz>,
    quest_board: Res<'w, QuestBoard>,
    activity: Res<'w, ActivityLog>,
    pawn_names: Query<'w, 's, &'static Name>,
    food_query: Query<'w, 's, (&'static Position, &'static FoodReservation)>,
    water_query: Query<'w, 's, (&'static Position, &'static WaterSource)>,
    audit_log: Res<'w, HypergraphAuditLog>,
}

#[derive(Default)]
struct LiveStatsPanelLocal {
    prev_time: f32,
    prev_tick: u64,
    ticks_per_sec: f32,
    fps: f32,
    prev_tier_metric_len: usize,
    thermal_delta_samples: VecDeque<(f32, f32)>,
    hyper_rewrite_samples: VecDeque<(f32, u64)>,
    tier4_birth_death_samples: VecDeque<(f32, u64, u64)>,
}

#[derive(Resource, Default)]
struct HypergraphRuntimeStats {
    rewritten_total: u64,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct HypergraphAuditEntry {
    tick: u64,
    elapsed_secs: f32,
    rewrite_count: u64,
    description: String,
}

#[derive(Resource, Default)]
struct HypergraphAuditLog {
    entries: VecDeque<HypergraphAuditEntry>,
}

impl HypergraphAuditLog {
    fn push(&mut self, entry: HypergraphAuditEntry) {
        self.entries.push_back(entry);
        while self.entries.len() > 50 {
            self.entries.pop_front();
        }
    }

    fn last_n(&self, n: usize) -> Vec<HypergraphAuditEntry> {
        self.entries.iter().rev().take(n).cloned().collect()
    }
}



#[derive(Resource, Default)]
struct VisualDebugThermalCache {
    prev_temp_by_chunk: HashMap<ChunkId, f32>,
}

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

fn benchmark_seconds_from_args() -> f64 {
    *BENCHMARK_SECONDS.get_or_init(|| {
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--benchmark-seconds" {
                let raw = args
                    .next()
                    .expect("missing value for --benchmark-seconds; provide a positive number");
                let parsed: f64 = raw
                    .parse()
                    .expect("invalid --benchmark-seconds value; provide a positive number");
                assert!(parsed > 0.0, "--benchmark-seconds must be > 0");
                return parsed;
            }
        }
        HEADLESS_MAX_WALL_SECONDS
    })
}

fn interactive_tier1_setting_from_flags(
    has_visual_debug_only: bool,
    has_interactive_with_tier1: bool,
) -> Result<bool, &'static str> {
    if has_visual_debug_only && has_interactive_with_tier1 {
        Err("invalid flags: use either --visual-debug-only or --interactive-with-tier1, not both")
    } else {
        Ok(has_interactive_with_tier1)
    }
}

fn interactive_with_tier1_from_args() -> bool {
    *INTERACTIVE_WITH_TIER1.get_or_init(|| {
        let has_visual_debug_only = std::env::args().skip(1).any(|arg| arg == "--visual-debug-only");
        let has_interactive_with_tier1 = std::env::args().skip(1).any(|arg| arg == "--interactive-with-tier1");
        match interactive_tier1_setting_from_flags(has_visual_debug_only, has_interactive_with_tier1) {
            Ok(value) => value,
            Err(message) => panic!("{}", message),
        }
    })
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
    SubstrateEquilibriumReached,
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
    let visual_debug_default_on = true;
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
        .init_resource::<HypergraphRuntimeStats>()
        .init_resource::<HypergraphAuditLog>()
        .init_resource::<ChemistryState>()
        .init_resource::<ThermalState>()
        .init_resource::<Tier4State>()
        .init_resource::<RespawnState>()
        .init_resource::<SimLifeState>()
        .insert_resource(VisualDebug {
            enabled: visual_debug_default_on,
        })
        .insert_resource(SimulationModeRuntime {
            mode: SimulationMode::Interactive,
            interactive_with_tier1: interactive_with_tier1_from_args(),
        })
        .init_resource::<VisualDebugThermalCache>()
        .add_plugins(MirrorPlugin);
    #[cfg(debug_assertions)]
    app.init_resource::<HypergraphDebugViz>();
    // Alpha: panic on any ECS error in every Bevy world, including plugin-created sub-apps.
    // Some plugins create or reconfigure sub-app worlds during build, so overwrite all handlers
    // only after plugin registration is complete and before any schedule runs.
    force_panic_error_handlers(&mut app);
    app.add_systems(Startup, (setup_hud_font, setup).chain())
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
                hypergraph_tick_system.run_if(run_hypergraph_tick_preupdate),
                simlife_tick_system,
                tier4_tick_system,
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
            ),
        )
        .add_systems(
            Update,
            (
                pawn_dominant_drive_color_system,
                charter_flash_spawn_system,
                charter_flash_tick_system,
                hypergraph_debug_input_system,
                hypergraph_debug_viz_system,
            ),
        )
        .add_systems(
            Update,
            (
                visual_debug_toggle_system,
                visual_debug_insect_overlay_system,
                visual_debug_gs_overlay_system,
            ),
        )
        .add_systems(
            Update,
            (activate_visual_debug_hypergraph_system, hypergraph_tick_system)
                .chain()
                .run_if(run_hypergraph_tick_visual_debug_update),
        )
        .add_systems(Update, visual_debug_thermal_overlay_system)
        .add_systems(Update, visual_debug_hypergraph_overlay_system)
        .add_systems(Update, live_stats_overlay_system);

    // Phase 4.D1: Push ECS snapshot to DuckDB mirror after Update (post visual sync).
    // Runs in its own add_systems call so it is not constrained by the Update chain tuple limit.
    app.add_systems(Update, push_ecs_snapshot_system);

    #[cfg(debug_assertions)]
    app.add_systems(Update, charter_watchguard_system);

    app.run();
}

fn setup_hud_font(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    // Use a vendored in-repo TTF to keep HUD rendering deterministic and independent of host OS fonts.
    let font_bytes = include_bytes!("../../assets/fonts/SimrardHUD-Regular.ttf").to_vec();
    let font = Font::try_from_bytes(font_bytes).unwrap_or_else(|err| {
        panic!(
            "HUD font load failed: bundled font at assets/fonts/SimrardHUD-Regular.ttf is invalid ({err}). Replace the file with a valid TTF.",
        )
    });
    commands.insert_resource(HudFontHandle(fonts.add(font)));
}

fn main() {
    run();
}

fn tier4_tick_system(
    mut state: ResMut<Tier4State>,
    mut simlife: ResMut<SimLifeState>,
    mut chemistry: ResMut<ChemistryState>,
    mut thermal: ResMut<ThermalState>,
    hypergraph: Option<Res<HypergraphSubstrate>>,
    mut charter: ResMut<SpatialCharter>,
    mut report: Option<ResMut<SimulationReport>>,
    global_clock: Res<GlobalTickClock>,
) {
    let current_seq = global_clock.causal_seq();
    let sink_temperature_k = thermal.sink_temperature_k;
    let metrics = advance_tier4(
        current_seq,
        &mut simlife.grass_per_chunk,
        &mut chemistry.receptor_noise_floor_by_chunk,
        &mut thermal.local_temperature_by_chunk,
        sink_temperature_k,
        hypergraph.as_deref(),
        &mut charter,
        &mut state,
    );
    if let Some(metrics) = metrics {
        if let Some(ref mut report) = report {
            report.counters.insert("tier4_population", metrics.population as u64);
            report.counters.insert("tier4_eat_grants", metrics.eat_grants);
            report.counters.insert("tier4_eat_denials", metrics.eat_denials);
            report.counters.insert("tier4_repro_grants", metrics.repro_grants);
            report.counters.insert("tier4_repro_denials", metrics.repro_denials);
            report.counters.insert("tier4_deaths", metrics.deaths);
            report.counters.insert("tier4_avg_energy", metrics.avg_energy as u64);
            report.counters.insert("tier4_decomp_total", state.decomp_total);
        }
    }
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
    let benchmark_seconds = benchmark_seconds_from_args();

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
        .init_resource::<HypergraphRuntimeStats>()
        .init_resource::<HypergraphAuditLog>()
        .init_resource::<ChemistryState>()
        .init_resource::<ThermalState>()
        .init_resource::<RespawnState>()
        .init_resource::<SimLifeState>()
        .init_resource::<SimulationReport>()
        .init_resource::<simrard_lib_tier4::Tier4State>()
        .insert_resource(SimulationLogSettings { stdout_enabled: false });

    match profile {
        HeadlessProfile::Full => {
            app.add_plugins(BigBrainPlugin::new(PreUpdate))
                .add_plugins(PawnAIPlugin)
                .init_resource::<QuestBoard>()
                .insert_resource(SimulationModeRuntime {
                    mode: SimulationMode::Headless,
                    interactive_with_tier1: true,
                })
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
                .insert_resource(SimulationModeRuntime {
                    mode: SimulationMode::Headless,
                    interactive_with_tier1: false,
                })
                .init_resource::<SubstrateStabilityState>()
                .add_systems(
                    Startup,
                    (initialize_substrate_baseline, initialize_substrate_activation).chain(),
                )
                .add_systems(
                    PreUpdate,
                    (
                        substrate_tick_driver,
                        hypergraph_tick_system,
                        simlife_tick_system,
                        tier4_tick_system,
                        thermal_passive_cooling_system,
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
                if profile == HeadlessProfile::Full && tick >= target_ticks {
                    break HeadlessTermination::TickLimitReached;
                }
                if started.elapsed().as_secs_f64() >= benchmark_seconds {
                    break HeadlessTermination::WallTimeLimitReached;
                }
                if profile == HeadlessProfile::Full && count_living_pawns(app.world_mut()) == 0 {
                    break HeadlessTermination::AllPawnsDied;
                }
                if profile == HeadlessProfile::SubstrateOnly {
                    if !ISOLATION_DISABLE_T60_AND_EXTRA_COUNTERS
                        && started.elapsed().as_secs_f64() >= 60.0
                    {
                        update_substrate_t60_debug_line(app.world_mut());
                    }
                    if let Some(stability) = app.world().get_resource::<SubstrateStabilityState>() {
                        if stability.equilibrium_reached {
                            break HeadlessTermination::SubstrateEquilibriumReached;
                        }
                    }
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

fn update_substrate_t60_debug_line(world: &mut World) {
    let rewrites_total = match world
        .get_resource::<SimulationReport>()
        .and_then(|report| report.counters.get("hypergraph_rewrites").copied())
    {
        Some(value) => value,
        None => 0,
    };
    let rule_fires = match world
        .get_resource::<SimulationReport>()
        .and_then(|report| report.counters.get("hypergraph_rule_fires").copied())
    {
        Some(value) => value,
        None => 0,
    };
    let cfg = world
        .get_resource::<HypergraphSubstrate>()
        .expect("HypergraphSubstrate missing in headless substrate profile; fix initialization")
        .config();
    let patch_count = (cfg.patch_cols as f64 * cfg.patch_rows as f64).max(1.0);
    let rewrites_per_patch_per_sec = rewrites_total as f64 / (patch_count * 60.0);
    let line = format!(
        "t=60s Hypergraph: rewrites_total={} avg_rewrites_per_patch_per_sec={:.4} active_rule_firing_count={}",
        rewrites_total,
        rewrites_per_patch_per_sec,
        rule_fires,
    );

    if let Some(mut state) = world.get_resource_mut::<SubstrateStabilityState>() {
        if state.debug_line_t60_hypergraph.is_none() {
            state.debug_line_t60_hypergraph = Some(line);
        }
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

fn initialize_substrate_activation(
    mut simlife: ResMut<SimLifeState>,
    mut chemistry: ResMut<ChemistryState>,
    mut charter: ResMut<SpatialCharter>,
    mut substrate: ResMut<HypergraphSubstrate>,
    mut state: ResMut<SubstrateStabilityState>,
) {
    // Substrate-only profile requires visible Tier 10 activity in the first minute.
    if ISOLATION_REVERT_HYPERGRAPH_CADENCE {
        substrate.set_interval_ticks(SUBSTRATE_HYPERGRAPH_INTERVAL_TICKS_PRE_TUNE);
    } else {
        substrate.set_interval_ticks(SUBSTRATE_HYPERGRAPH_INTERVAL_TICKS);
    }
    substrate.set_chaos(SUBSTRATE_HYPERGRAPH_CHAOS);

    let seeded = gs_seed_initial_state(0, &mut simlife, Some(&mut chemistry), &mut charter);

    let total_cells = ((CHUNK_EXTENT as u64) + 1).pow(2) as f32;
    let non_zero_cells = simlife
        .v_field
        .iter()
        .filter(|(_, v)| **v > 0.0)
        .count();
    let (sum_u, sum_v, sample_count) = simlife
        .u_field
        .iter()
        .filter_map(|(chunk, u)| {
            simlife
                .v_field
                .get(chunk)
                .map(|v| (*u as f64, *v as f64))
        })
        .fold((0.0_f64, 0.0_f64, 0_u64), |(su, sv, c), (u, v)| {
            (su + u, sv + v, c + 1)
        });
    let avg_u = if sample_count > 0 {
        sum_u / sample_count as f64
    } else {
        0.0
    };
    let avg_v = if sample_count > 0 {
        sum_v / sample_count as f64
    } else {
        0.0
    };
    let coverage_pct = if total_cells > 0.0 {
        (non_zero_cells as f32 / total_cells) * 100.0
    } else {
        0.0
    };

    let fungal_coverage_pct = if total_cells > 0.0 {
        (simlife.grass_per_chunk.len() as f32 / total_cells) * 100.0
    } else {
        0.0
    };
    let nutrient_redistribution_rate = if seeded == 0 {
        0.0
    } else {
        let mut total = 0.0_f64;
        for chunk in simlife.v_field.keys() {
                let noise_floor = match chemistry.receptor_noise_floor_by_chunk.get(chunk).copied() {
                    Some(value) => value,
                    None => 0.0,
                };
                total += noise_floor as f64 / HYPERGRAPH_NOISE_FLOOR_MAX as f64;
        }
        total / seeded as f64
    };

    state.debug_line_t0_gray_scott = format!(
        "t=0 Gray-Scott seed: non_zero_cells={} avg_u={:.4} avg_v={:.4} coverage={:.4}%",
        non_zero_cells, avg_u, avg_v, coverage_pct
    );
    state.debug_line_t0_fungal = format!(
        "t=0 Fungal init: coverage={:.4}% nutrient_redistribution_rate={:.4}",
        fungal_coverage_pct, nutrient_redistribution_rate
    );
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
    start_histogram: Option<[u64; 32]>,
    end_histogram: Option<[u64; 32]>,
    second_samples: Vec<SubstrateSecondSample>,
    tier_scores: TierStabilityScores,
    overall_vitality: f64,
    equilibrium_reached: bool,
    equilibrium_tier: Option<&'static str>,
    equilibrium_seconds: Option<f64>,
    // Per-tier low-activity streak counters: index [0]=T5, [1]=T6, [2]=T7, [3]=T8, [4]=T9, [5]=T10, [6]=T4.
    tier_low_streak: [u64; 7],
    debug_line_t0_gray_scott: String,
    debug_line_t0_fungal: String,
    debug_line_t60_hypergraph: Option<String>,
}

#[derive(Debug, Clone)]
struct SubstrateSecondSample {
    second: u64,
    rewrites_total: u64,
    chemistry_hist_32: [u64; 32],
    coverage_pct: f32,
    uv_mass_norm: f32,
    energy_flux: f32,
    heat_dissipated_total: f32,
    tier4_population: u64,
}

#[derive(Debug, Clone, Copy)]
struct TierStabilityScores {
    tier4_reflex: f64,
    tier10_hypergraph: f64,
    tier9_energy: f64,
    tier8_mineral: f64,
    tier7_chemistry: f64,
    tier6_fungal: f64,
    tier5_vegetable: f64,
}

impl Default for TierStabilityScores {
    fn default() -> Self {
        Self {
            tier4_reflex: 0.0,
            tier10_hypergraph: 0.0,
            tier9_energy: 0.0,
            tier8_mineral: 0.0,
            tier7_chemistry: 0.0,
            tier6_fungal: 0.0,
            tier5_vegetable: 0.0,
        }
    }
}

impl Default for SubstrateStabilityState {
    fn default() -> Self {
        Self {
            last_tick: 0,
            start_coverage_pct: None,
            end_coverage_pct: None,
            start_histogram: None,
            end_histogram: None,
            second_samples: Vec::new(),
            tier_scores: TierStabilityScores::default(),
            overall_vitality: 0.0,
            equilibrium_reached: false,
            equilibrium_tier: None,
            equilibrium_seconds: None,
            tier_low_streak: [0; 7],
            debug_line_t0_gray_scott: "t=0 Gray-Scott seed: unavailable".to_string(),
            debug_line_t0_fungal: "t=0 Fungal init: unavailable".to_string(),
            debug_line_t60_hypergraph: None,
        }
    }
}

fn histogram_entropy_32(hist: &[u64; 32]) -> f64 {
    let total: u64 = hist.iter().sum();
    if total == 0 {
        return 0.0;
    }
    let total_f = total as f64;
    let mut entropy = 0.0;
    for value in hist {
        if *value == 0 {
            continue;
        }
        let p = *value as f64 / total_f;
        entropy -= p * p.ln();
    }
    let max_entropy = (32_f64).ln();
    if max_entropy <= 0.0 {
        0.0
    } else {
        (entropy / max_entropy).clamp(0.0, 1.0)
    }
}

fn variance(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    values
        .iter()
        .map(|value| {
            let d = *value - mean;
            d * d
        })
        .sum::<f64>()
        / values.len() as f64
}

fn max_hist_bin_change_ratio(window: &[SubstrateSecondSample]) -> f64 {
    if window.len() < 2 {
        return 0.0;
    }
    let mut max_change = 0.0_f64;
    for pair in window.windows(2) {
        let a = &pair[0].chemistry_hist_32;
        let b = &pair[1].chemistry_hist_32;
        let total_a: u64 = a.iter().sum();
        let total_b: u64 = b.iter().sum();
        let norm_total = total_a.max(total_b) as f64;
        if norm_total == 0.0 {
            continue;
        }
        for (x, y) in a.iter().zip(b.iter()) {
            let change = x.abs_diff(*y) as f64 / norm_total;
            if change > max_change {
                max_change = change;
            }
        }
    }
    max_change
}

fn max_uv_change_ratio(window: &[SubstrateSecondSample]) -> f64 {
    if window.len() < 2 {
        return 0.0;
    }
    let mut max_change = 0.0_f64;
    for pair in window.windows(2) {
        let a = pair[0].uv_mass_norm as f64;
        let b = pair[1].uv_mass_norm as f64;
        let change = (b - a).abs();
        if change > max_change {
            max_change = change;
        }
    }
    max_change
}

// Collapse thresholds (calibrated): a tier is in equilibrium when its raw activity stays
// below the threshold for COLLAPSE_STREAK_REQUIRED consecutive probe seconds.
const T10_COLLAPSE_THRESHOLD: f64 = 0.001;
const T9_COLLAPSE_THRESHOLD: f64 = 0.005;
const T8_COLLAPSE_THRESHOLD: f64 = 0.02;
const T7_COLLAPSE_THRESHOLD: f64 = 0.002;
const T6_COLLAPSE_THRESHOLD: f64 = 0.0005;
const T5_COLLAPSE_THRESHOLD: f64 = 0.001;
const T4_COLLAPSE_THRESHOLD: f64 = 0.005;
const COLLAPSE_STREAK_REQUIRED: u64 = 30;

fn compute_tier_scores(state: &mut SubstrateStabilityState) {
    if state.second_samples.is_empty() {
        return;
    }
    let latest = state
        .second_samples
        .last()
        .expect("second_samples must be non-empty when computing tier scores");
    let window = if state.second_samples.len() > 30 {
        &state.second_samples[state.second_samples.len() - 30..]
    } else {
        &state.second_samples[..]
    };

    // Tier 10: rewrites/sec from cumulative counter derivative.
    let rewrites_per_second = if state.second_samples.len() < 2 {
        0.0
    } else {
        let prev = &state.second_samples[state.second_samples.len() - 2];
        latest.rewrites_total.saturating_sub(prev.rewrites_total) as f64
    };
    state.tier_scores.tier10_hypergraph = (rewrites_per_second / 10.0).clamp(0.0, 1.0);
    if rewrites_per_second < T10_COLLAPSE_THRESHOLD {
        state.tier_low_streak[5] = state.tier_low_streak[5].saturating_add(1);
    } else {
        state.tier_low_streak[5] = 0;
    }

    // Tier 9: variance of recent sink dissipation throughput.
    let energy_fluxes: Vec<f64> = window.iter().map(|s| s.energy_flux as f64).collect();
    let flux_variance = variance(&energy_fluxes);
    state.tier_scores.tier9_energy = (flux_variance / 0.1).clamp(0.0, 1.0);
    if flux_variance < T9_COLLAPSE_THRESHOLD {
        state.tier_low_streak[4] = state.tier_low_streak[4].saturating_add(1);
    } else {
        state.tier_low_streak[4] = 0;
    }

    // Tier 8: histogram entropy normalized by ln(32).
    let entropy_norm = histogram_entropy_32(&latest.chemistry_hist_32);
    state.tier_scores.tier8_mineral = entropy_norm;
    if entropy_norm < T8_COLLAPSE_THRESHOLD {
        state.tier_low_streak[3] = state.tier_low_streak[3].saturating_add(1);
    } else {
        state.tier_low_streak[3] = 0;
    }

    // Tier 7: max chemistry histogram bin change over last 30s.
    let max_bin_change = max_hist_bin_change_ratio(window);
    state.tier_scores.tier7_chemistry = (max_bin_change / 0.01).clamp(0.0, 1.0);
    if max_bin_change < T7_COLLAPSE_THRESHOLD {
        state.tier_low_streak[2] = state.tier_low_streak[2].saturating_add(1);
    } else {
        state.tier_low_streak[2] = 0;
    }

    // Tier 6: fungal coverage change rate over last 30s.
    let coverage_change_rate = if window.len() < 2 {
        0.0
    } else {
        let first = window
            .first()
            .expect("window first sample missing");
        let last = window
            .last()
            .expect("window last sample missing");
        let seconds = (last.second.saturating_sub(first.second)).max(1) as f64;
        (last.coverage_pct as f64 - first.coverage_pct as f64).abs() / seconds
    };
    state.tier_scores.tier6_fungal = (coverage_change_rate / 0.015).clamp(0.0, 1.0);
    if coverage_change_rate < T6_COLLAPSE_THRESHOLD {
        state.tier_low_streak[1] = state.tier_low_streak[1].saturating_add(1);
    } else {
        state.tier_low_streak[1] = 0;
    }

    // Tier 5: max U+V field change over last 30s.
    let max_uv_change = max_uv_change_ratio(window);
    state.tier_scores.tier5_vegetable = (max_uv_change / 0.0004).clamp(0.0, 1.0);
    if max_uv_change < T5_COLLAPSE_THRESHOLD {
        state.tier_low_streak[0] = state.tier_low_streak[0].saturating_add(1);
    } else {
        state.tier_low_streak[0] = 0;
    }

    // Tier 4: normalized reflex-insect population vitality.
    let tier4_population_norm = (latest.tier4_population as f64 / 10_000.0).clamp(0.0, 1.0);
    state.tier_scores.tier4_reflex = tier4_population_norm;
    if tier4_population_norm < T4_COLLAPSE_THRESHOLD {
        state.tier_low_streak[6] = state.tier_low_streak[6].saturating_add(1);
    } else {
        state.tier_low_streak[6] = 0;
    }

    state.overall_vitality = (
        state.tier_scores.tier10_hypergraph
            + state.tier_scores.tier9_energy
            + state.tier_scores.tier8_mineral
            + state.tier_scores.tier7_chemistry
            + state.tier_scores.tier6_fungal
            + state.tier_scores.tier5_vegetable
            + state.tier_scores.tier4_reflex
    ) / 7.0;

    let all_equilibrium = state
        .tier_low_streak
        .iter()
        .all(|streak| *streak >= COLLAPSE_STREAK_REQUIRED);

    if all_equilibrium && !state.equilibrium_reached {
        state.equilibrium_reached = true;
        // Bottleneck tier = currently most dynamic among the six when equilibrium is first reached.
        let mut tiers = [
            ("Tier 10", state.tier_scores.tier10_hypergraph),
            ("Tier 9", state.tier_scores.tier9_energy),
            ("Tier 8", state.tier_scores.tier8_mineral),
            ("Tier 7", state.tier_scores.tier7_chemistry),
            ("Tier 6", state.tier_scores.tier6_fungal),
            ("Tier 5", state.tier_scores.tier5_vegetable),
            ("Tier 4", state.tier_scores.tier4_reflex),
        ];
        tiers.sort_by(|a, b| match b.1.partial_cmp(&a.1) {
            Some(ordering) => ordering,
            None => Ordering::Equal,
        });
        state.equilibrium_tier = Some(tiers[0].0);
        state.equilibrium_seconds = Some(latest.second as f64);
    }
}

fn tier_trend(earlier_avg: f64, later_avg: f64) -> &'static str {
    const TREND_THRESHOLD: f64 = 0.1;
    if earlier_avg < 1e-10 && later_avg < 1e-10 {
        return "stable";
    }
    if earlier_avg < 1e-10 {
        return "rising";
    }
    let ratio = later_avg / earlier_avg;
    if ratio > 1.0 + TREND_THRESHOLD {
        "rising"
    } else if ratio < 1.0 - TREND_THRESHOLD {
        "falling"
    } else {
        "stable"
    }
}

fn window_avg_rewrites_per_sec(samples: &[SubstrateSecondSample]) -> f64 {
    if samples.len() < 2 {
        return 0.0;
    }
    let first = &samples[0];
    let last = &samples[samples.len() - 1];
    let span = last.second.saturating_sub(first.second) as f64;
    if span < 1.0 {
        return 0.0;
    }
    last.rewrites_total.saturating_sub(first.rewrites_total) as f64 / span
}

struct TierTrends {
    tier4: &'static str,
    tier10: &'static str,
    tier9: &'static str,
    tier8: &'static str,
    tier7: &'static str,
    tier6: &'static str,
    tier5: &'static str,
}

fn compute_tier_trends(samples: &[SubstrateSecondSample]) -> TierTrends {
    if samples.len() < 6 {
        let s = "stable";
        return TierTrends {
            tier4: s,
            tier10: s,
            tier9: s,
            tier8: s,
            tier7: s,
            tier6: s,
            tier5: s,
        };
    }
    let mid = samples.len() / 2;
    let early = &samples[..mid];
    let late = &samples[mid..];

    let early_entropy: f64 = early
        .iter()
        .map(|s| histogram_entropy_32(&s.chemistry_hist_32))
        .sum::<f64>()
        / early.len() as f64;
    let late_entropy: f64 = late
        .iter()
        .map(|s| histogram_entropy_32(&s.chemistry_hist_32))
        .sum::<f64>()
        / late.len() as f64;

    let early_cov_change = if early.len() < 2 {
        0.0
    } else {
        let span = early
            .last()
            .expect("early half last sample missing")
            .second
            .saturating_sub(early[0].second)
            .max(1) as f64;
        (early
            .last()
            .expect("early half last sample missing")
            .coverage_pct as f64
            - early[0].coverage_pct as f64)
            .abs()
            / span
    };
    let late_cov_change = if late.len() < 2 {
        0.0
    } else {
        let span = late
            .last()
            .expect("late half last sample missing")
            .second
            .saturating_sub(late[0].second)
            .max(1) as f64;
        (late
            .last()
            .expect("late half last sample missing")
            .coverage_pct as f64
            - late[0].coverage_pct as f64)
            .abs()
            / span
    };

    let early_fluxes: Vec<f64> = early.iter().map(|s| s.energy_flux as f64).collect();
    let late_fluxes: Vec<f64> = late.iter().map(|s| s.energy_flux as f64).collect();

    let early_tier9 = (variance(&early_fluxes) / 0.1).clamp(0.0, 1.0);
    let late_tier9 = (variance(&late_fluxes) / 0.1).clamp(0.0, 1.0);
    let early_tier4 = early
        .iter()
        .map(|s| s.tier4_population as f64 / 10_000.0)
        .sum::<f64>()
        / early.len() as f64;
    let late_tier4 = late
        .iter()
        .map(|s| s.tier4_population as f64 / 10_000.0)
        .sum::<f64>()
        / late.len() as f64;

    TierTrends {
        tier4: tier_trend(early_tier4, late_tier4),
        tier10: tier_trend(
            window_avg_rewrites_per_sec(early),
            window_avg_rewrites_per_sec(late),
        ),
        tier9: tier_trend(early_tier9, late_tier9),
        tier8: tier_trend(early_entropy, late_entropy),
        tier7: tier_trend(
            max_hist_bin_change_ratio(early),
            max_hist_bin_change_ratio(late),
        ),
        tier6: tier_trend(early_cov_change, late_cov_change),
        tier5: tier_trend(max_uv_change_ratio(early), max_uv_change_ratio(late)),
    }
}

fn tier_status(streak: u64, vitality: f64) -> &'static str {
    if vitality > 0.3 {
        "Active"
    } else if vitality >= 0.1 {
        "Collapsing"
    } else if streak >= COLLAPSE_STREAK_REQUIRED {
        "Collapsed"
    } else {
        "Collapsing"
    }
}

fn substrate_stability_probe_system(
    global_clock: Res<GlobalTickClock>,
    simlife: Res<SimLifeState>,
    chemistry: Res<ChemistryState>,
    report: Res<SimulationReport>,
    substrate: Res<HypergraphSubstrate>,
    thermal: Res<ThermalState>,
    mut state: ResMut<SubstrateStabilityState>,
) {
    let seq = global_clock.causal_seq();
    if seq <= state.last_tick {
        return;
    }
    state.last_tick = seq;

    // Probe once per simulated second to produce 30-second rolling windows.
    if seq % 1000 != 0 {
        return;
    }

    if ISOLATION_DISABLE_PER_SECOND_PROBES {
        return;
    }

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

    let mut histogram = [0_u64; 32];
    for value in chemistry.receptor_noise_floor_by_chunk.values() {
        let normalized = (*value / HYPERGRAPH_NOISE_FLOOR_MAX).clamp(0.0, 0.9999);
        let bin = (normalized * histogram.len() as f32).floor() as usize;
        histogram[bin] += 1;
    }

    let heat_dissipated_total = thermal.cumulative_heat_dissipated as f32;
    let mut flux_sum = heat_dissipated_total;
    if let Some(previous) = state.second_samples.last() {
        flux_sum = (heat_dissipated_total - previous.heat_dissipated_total).max(0.0);
    }

    let uv_mass_norm = if total_cells > 0.0 {
        (simlife.u_field.values().copied().sum::<f32>() + simlife.v_field.values().copied().sum::<f32>()) / total_cells
    } else {
        0.0
    };

    let rewrites_total = match report.counters.get("hypergraph_rewrites") {
        Some(value) => *value,
        None => 0,
    };
    let tier4_population = match report.counters.get("tier4_population") {
        Some(value) => *value,
        None => 0,
    };

    state.second_samples.push(SubstrateSecondSample {
        second: seq / 1000,
        rewrites_total,
        chemistry_hist_32: histogram,
        coverage_pct,
        uv_mass_norm,
        energy_flux: flux_sum,
        heat_dissipated_total,
        tier4_population,
    });
    if state.second_samples.len() > 60 {
        state.second_samples.remove(0);
    }

    if state.start_coverage_pct.is_none() {
        state.start_coverage_pct = Some(coverage_pct);
    }
    if state.start_histogram.is_none() {
        state.start_histogram = Some(histogram);
    }
    state.end_coverage_pct = Some(coverage_pct);
    state.end_histogram = Some(histogram);

    compute_tier_scores(&mut state);

    if !ISOLATION_DISABLE_T60_AND_EXTRA_COUNTERS
        && state.debug_line_t60_hypergraph.is_none()
        && (seq / 1000) >= 60
    {
        let rewrites_total = match report.counters.get("hypergraph_rewrites").copied() {
            Some(value) => value,
            None => 0,
        };
        let rule_fires = match report.counters.get("hypergraph_rule_fires").copied() {
            Some(value) => value,
            None => 0,
        };
        let cfg = substrate.config();
        let patch_count = (cfg.patch_cols as f64 * cfg.patch_rows as f64).max(1.0);
        let rewrites_per_patch_per_sec = rewrites_total as f64 / (patch_count * 60.0);
        state.debug_line_t60_hypergraph = Some(format!(
            "t=60s Hypergraph: rewrites_total={} avg_rewrites_per_patch_per_sec={:.4} active_rule_firing_count={}",
            rewrites_total,
            rewrites_per_patch_per_sec,
            rule_fires,
        ));
    }
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
        HeadlessTermination::SubstrateEquilibriumReached => {
            format!("Substrate equilibrium reached at {} ticks.", tick)
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
            let samples = &stability.second_samples;
            let window = if samples.len() > 30 {
                &samples[samples.len() - 30..]
            } else {
                &samples[..]
            };
            let trends = compute_tier_trends(window);

            lines.push("=== Tier Monitor ===".to_string());
            lines.push(format!(
                "  {:>4}  {:>8}  {:^11}  {}",
                "Tier", "Vitality", "Trend(30s)", "Status"
            ));
            lines.push(format!(
                "  {:>4}  {:>8}  {:^11}  {}",
                "----", "--------", "-----------", "-------"
            ));
            lines.push(format!(
                "  {:>4}  {:>8.4}  {:^11}  {}",
                "10",
                stability.tier_scores.tier10_hypergraph,
                trends.tier10,
                tier_status(stability.tier_low_streak[5], stability.tier_scores.tier10_hypergraph),
            ));
            lines.push(format!(
                "  {:>4}  {:>8.4}  {:^11}  {}",
                "9",
                stability.tier_scores.tier9_energy,
                trends.tier9,
                tier_status(stability.tier_low_streak[4], stability.tier_scores.tier9_energy),
            ));
            lines.push(format!(
                "  {:>4}  {:>8.4}  {:^11}  {}",
                "8",
                stability.tier_scores.tier8_mineral,
                trends.tier8,
                tier_status(stability.tier_low_streak[3], stability.tier_scores.tier8_mineral),
            ));
            lines.push(format!(
                "  {:>4}  {:>8.4}  {:^11}  {}",
                "7",
                stability.tier_scores.tier7_chemistry,
                trends.tier7,
                tier_status(stability.tier_low_streak[2], stability.tier_scores.tier7_chemistry),
            ));
            lines.push(format!(
                "  {:>4}  {:>8.4}  {:^11}  {}",
                "6",
                stability.tier_scores.tier6_fungal,
                trends.tier6,
                tier_status(stability.tier_low_streak[1], stability.tier_scores.tier6_fungal),
            ));
            lines.push(format!(
                "  {:>4}  {:>8.4}  {:^11}  {}",
                "5",
                stability.tier_scores.tier5_vegetable,
                trends.tier5,
                tier_status(stability.tier_low_streak[0], stability.tier_scores.tier5_vegetable),
            ));
            lines.push(format!(
                "  {:>4}  {:>8.4}  {:^11}  {}",
                "4",
                stability.tier_scores.tier4_reflex,
                trends.tier4,
                tier_status(stability.tier_low_streak[6], stability.tier_scores.tier4_reflex),
            ));

            let vitality_status = if stability.equilibrium_reached {
                let tier = match stability.equilibrium_tier {
                    Some(value) => value,
                    None => "Tier 10",
                };
                let seconds = match stability.equilibrium_seconds {
                    Some(value) => value,
                    None => elapsed_secs,
                };
                format!(
                    "  Overall vitality: {:.2}  (Equilibrium Reached at {} after {:.1}s)",
                    stability.overall_vitality, tier, seconds
                )
            } else {
                format!(
                    "  Overall vitality: {:.2}  (still dynamic)",
                    stability.overall_vitality
                )
            };
            lines.push(vitality_status);

            lines.push(format!("  {}", stability.debug_line_t0_gray_scott));
            lines.push(format!("  {}", stability.debug_line_t0_fungal));
            let t60_line = match stability.debug_line_t60_hypergraph.clone() {
                Some(value) => value,
                None => "t=60s Hypergraph: unavailable".to_string(),
            };
            lines.push(format!("  {}", t60_line));

            let any_tier_equilibrium = stability
                .tier_low_streak
                .iter()
                .any(|streak| *streak >= COLLAPSE_STREAK_REQUIRED);
            lines.push(format!(
                "  Any tier reached equilibrium: {}",
                if any_tier_equilibrium { "yes" } else { "no" }
            ));

            let start = match stability.start_coverage_pct {
                Some(value) => value,
                None => 0.0,
            };
            let end = match stability.end_coverage_pct {
                Some(value) => value,
                None => 0.0,
            };
            lines.push(format!(
                "  Fungal coverage: {:.4}% -> {:.4}% (delta {:+.4}%)",
                start, end, end - start
            ));

            let end_hist = match stability.end_histogram {
                Some(value) => value,
                None => [0; 32],
            };
            lines.push(format!(
                "  Final chemistry histogram entropy: {:.4}",
                histogram_entropy_32(&end_hist)
            ));
            if let Some(thermal) = world.get_resource::<ThermalState>() {
                lines.push(format!(
                    "  Thermal sink: {:.2}K with cooling_k={:.3}",
                    thermal.sink_temperature_k,
                    thermal.cooling_k,
                ));
                lines.push(format!(
                    "  Thermal hotspot: latest_peak={:.2}K peak_seen={:.2}K dissipated_total={:.2}",
                    thermal.latest_peak_temperature_k,
                    thermal.peak_temperature_seen_k,
                    thermal.cumulative_heat_dissipated,
                ));
                lines.push(format!(
                    "  Runaway boiling: {} / runaway freezing: {}",
                    if thermal.peak_temperature_seen_k >= THERMAL_RUNAWAY_BOIL_K { "yes" } else { "no" },
                    if thermal.latest_peak_temperature_k <= THERMAL_RUNAWAY_FREEZE_K { "yes" } else { "no" },
                ));
            }
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
            let top_n = if profile == HeadlessProfile::SubstrateOnly {
                5
            } else {
                3
            };
            lines.push(format!("Perf breakdown (top {}):", top_n));
            let denom = audit.headless_total.as_secs_f64().max(0.000_001);
            for (name, total, count) in entries.into_iter().take(top_n) {
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
fn world_bounds_pixels() -> (f32, f32, f32, f32) {
    (0.0, WORLD_EXTENT_PIXELS, 0.0, WORLD_EXTENT_PIXELS)
}

fn camera_viewport_bounds(transform: &Transform, ortho: &OrthographicProjection) -> (f32, f32, f32, f32) {
    (
        transform.translation.x + ortho.area.min.x,
        transform.translation.x + ortho.area.max.x,
        transform.translation.y + ortho.area.min.y,
        transform.translation.y + ortho.area.max.y,
    )
}

fn visible_world_bounds(
    viewport_min_x: f32,
    viewport_max_x: f32,
    viewport_min_y: f32,
    viewport_max_y: f32,
) -> Option<(f32, f32, f32, f32)> {
    let (world_min_x, world_max_x, world_min_y, world_max_y) = world_bounds_pixels();
    let vis_min_x = viewport_min_x.max(world_min_x);
    let vis_max_x = viewport_max_x.min(world_max_x);
    let vis_min_y = viewport_min_y.max(world_min_y);
    let vis_max_y = viewport_max_y.min(world_max_y);
    if vis_min_x >= vis_max_x || vis_min_y >= vis_max_y {
        None
    } else {
        Some((vis_min_x, vis_max_x, vis_min_y, vis_max_y))
    }
}

fn live_stats_panel_layout(
    cam_x: f32,
    cam_y: f32,
    area_max_x: f32,
    area_max_y: f32,
) -> (f32, f32, f32, f32, f32, f32) {
    let panel_w = 1650.0;
    let panel_h = 640.0;
    let margin_x = 220.0;
    let margin_y = 180.0;
    let text_padding_left = 64.0;
    let text_padding_top = 54.0;
    let panel_center_x = cam_x - area_max_x + margin_x + panel_w * 0.5;
    let panel_center_y = cam_y + area_max_y - margin_y - panel_h * 0.5;
    let text_x = panel_center_x - panel_w * 0.5 + text_padding_left;
    let text_y = panel_center_y + panel_h * 0.5 - text_padding_top;
    (panel_center_x, panel_center_y, text_x, text_y, panel_w, panel_h)
}

fn chunk_grid_gizmo_system(
    camera_query: Query<(&Transform, &Projection), With<Camera2d>>,
    mut gizmos: Gizmos,
) {
    let Ok((transform, projection)) = camera_query.single() else {
        return;
    };
    let ortho = match projection {
        Projection::Orthographic(value) => value,
        _ => return,
    };

    let (viewport_min_x, viewport_max_x, viewport_min_y, viewport_max_y) =
        camera_viewport_bounds(transform, ortho);
    let Some((vis_min_x, vis_max_x, vis_min_y, vis_max_y)) =
        visible_world_bounds(viewport_min_x, viewport_max_x, viewport_min_y, viewport_max_y)
    else {
        return;
    };

    let (world_min_x, world_max_x, world_min_y, world_max_y) = world_bounds_pixels();

    // Draw world boundary as a bright outline
    let border_color = Color::srgba(0.55, 0.65, 0.80, 0.90);
    gizmos.line_2d(Vec2::new(world_min_x, world_min_y), Vec2::new(world_max_x, world_min_y), border_color);
    gizmos.line_2d(Vec2::new(world_max_x, world_min_y), Vec2::new(world_max_x, world_max_y), border_color);
    gizmos.line_2d(Vec2::new(world_max_x, world_max_y), Vec2::new(world_min_x, world_max_y), border_color);
    gizmos.line_2d(Vec2::new(world_min_x, world_max_y), Vec2::new(world_min_x, world_min_y), border_color);

    // Draw interior grid lines only where visible and within world bounds
    let start_i = ((vis_min_x / CHUNK_PIXEL).floor() as i32).max(0);
    let end_i = ((vis_max_x / CHUNK_PIXEL).ceil() as i32).min(256);
    let start_j = ((vis_min_y / CHUNK_PIXEL).floor() as i32).max(0);
    let end_j = ((vis_max_y / CHUNK_PIXEL).ceil() as i32).min(256);

    for i in start_i..=end_i {
        let p = i as f32 * CHUNK_PIXEL;
        let major = i % 32 == 0;
        let color = if major {
            Color::srgba(0.42, 0.48, 0.62, 0.85)
        } else if i % 8 == 0 {
            Color::srgba(0.36, 0.40, 0.52, 0.72)
        } else {
            Color::srgba(0.30, 0.32, 0.40, 0.65)
        };
        gizmos.line_2d(Vec2::new(p, vis_min_y), Vec2::new(p, vis_max_y), color);
    }
    for j in start_j..=end_j {
        let p = j as f32 * CHUNK_PIXEL;
        let major = j % 32 == 0;
        let color = if major {
            Color::srgba(0.42, 0.48, 0.62, 0.85)
        } else if j % 8 == 0 {
            Color::srgba(0.36, 0.40, 0.52, 0.72)
        } else {
            Color::srgba(0.30, 0.32, 0.40, 0.65)
        };
        gizmos.line_2d(Vec2::new(vis_min_x, p), Vec2::new(vis_max_x, p), color);
    }
}

#[derive(Component)]
struct VisualDebugInsectSprite;

#[derive(Component)]
struct VisualDebugGsFullWorldTint;

#[derive(Component)]
struct VisualDebugThermalSprite;

#[derive(Component)]
struct LiveStatsPanelUiRoot;

#[derive(Component)]
struct LiveStatsPanelTextUi;

const VISUAL_DEBUG_MAX_INSECT_SPRITES: usize = 200;
const VISUAL_DEBUG_MAX_THERMAL_SPRITES: usize = 20;

fn visual_debug_toggle_system(keys: Res<ButtonInput<KeyCode>>, mut visual: ResMut<VisualDebug>) {
    if keys.just_pressed(KeyCode::KeyV) {
        visual.enabled = !visual.enabled;
    }
}

fn visual_debug_insect_overlay_system(
    mut commands: Commands,
    visual: Res<VisualDebug>,
    tier4: Res<Tier4State>,
    time: Res<Time>,
    camera_query: Query<&Transform, With<Camera2d>>,
    existing: Query<Entity, With<VisualDebugInsectSprite>>,
) {
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }
    if !visual.enabled || tier4.insects.is_empty() {
        return;
    }

    let camera = match camera_query.single() {
        Ok(value) => value,
        Err(_) => return,
    };
    let cam = Vec2::new(camera.translation.x, camera.translation.y);
    let mut order: Vec<(usize, f32)> = tier4
        .insects
        .iter()
        .enumerate()
        .map(|(idx, insect)| {
            let pos = Vec2::new(insect.chunk.0 as f32 * CHUNK_PIXEL, insect.chunk.1 as f32 * CHUNK_PIXEL);
            (idx, pos.distance_squared(cam))
        })
        .collect();
    order.sort_by(|a, b| match a.1.partial_cmp(&b.1) {
        Some(ordering) => ordering,
        None => Ordering::Equal,
    });

    let t = time.elapsed_secs();
    for (idx, _) in order.into_iter().take(VISUAL_DEBUG_MAX_INSECT_SPRITES) {
        let insect = &tier4.insects[idx];
        let hunger = insect.hunger.clamp(0.0, 1.0);
        let fear = insect.fear.clamp(0.0, 1.0);
        let sum = (hunger + fear).max(0.001);
        let hw = hunger / sum;
        let fw = fear / sum;
        let mut r = (0.95 * hw + 0.66 * fw).clamp(0.0, 1.0);
        let mut g = (0.12 * hw + 0.10 * fw).clamp(0.0, 1.0);
        let mut b = (0.18 * hw + 0.90 * fw).clamp(0.0, 1.0);
        
        let energy_norm = (insect.energy / 8.0).clamp(0.0, 1.0);
        
        // 0.8 Hz pulse for subtle scale bob + color pulse (red → bright-orange → red)
        let pulse_freq = 0.8;
        let pulse_rad = t * pulse_freq * std::f32::consts::TAU;
        let scale_pulse = pulse_rad.sin();
        
        // Scale bob: 0.9× to 1.1× amplitude
        let scale_modulation = 1.0 + 0.1 * scale_pulse;
        
        // Color pulse: blend toward bright orange on positive pulse
        // When pulse > 0: shift color toward orange (1.0, 0.6, 0.0)
        let orange_blend = scale_pulse.max(0.0);
        r = (1.0 - orange_blend) * r + orange_blend * 1.0;
        g = (1.0 - orange_blend) * g + orange_blend * 0.6;
        b = (1.0 - orange_blend) * b + orange_blend * 0.0;
        
        let phase = insect.age as f32 * 0.11 + insect.chunk.0 as f32 * 0.03 + insect.chunk.1 as f32 * 0.02;
        let base_size = 4.2 + insect.energy.clamp(0.0, 8.0) * 1.2;
        let width = base_size * scale_modulation;
        let height = base_size * scale_modulation;
        
        let mut pos = chunk_to_translation(&insect.chunk, 3.2);
        pos.x += ((insect.age as f32).sin() * 0.35).clamp(-0.4, 0.4);
        pos.y += ((insect.age as f32 * 0.73).cos() * 0.35).clamp(-0.4, 0.4);
        pos = clip_to_world_bounds(pos);
        
        let leg_wobble = 0.28 * (t * (6.0 + hunger * 4.0) + phase * 0.7).sin();
        let alpha = (0.82 + 0.14 * energy_norm + 0.06 * scale_pulse).clamp(0.55, 1.0);
        
        commands.spawn((
            VisualDebugInsectSprite,
            Sprite::from_color(Color::srgba(r, g, b, alpha), Vec2::new(width, height)),
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_z(leg_wobble)),
        ));
    }
}

fn visual_debug_gs_overlay_system(
    mut commands: Commands,
    visual: Res<VisualDebug>,
    simlife: Res<SimLifeState>,
    existing: Query<Entity, With<VisualDebugGsFullWorldTint>>,
) {
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }
    if !visual.enabled || simlife.gs_active.is_empty() {
        return;
    }

    // Compute average v-concentration (biomass) across all active GS cells
    let mut total_v = 0.0;
    let mut count = 0;
    for cell in simlife.gs_active.iter() {
        if let Some(v) = simlife.v_field.get(cell) {
            total_v += *v;
            count += 1;
        }
    }
    
    let avg_v = if count > 0 {
        (total_v / count as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    
    // Map concentration to blue-green tint:
    // - Low concentration (0.0): transparent
    // - High concentration (1.0): bright blue-green
    // Use v as the dominant indicator of biomass spreading
    let r = 0.1 * avg_v;  // Minimal red component
    let g = 0.7 * avg_v;  // Strong green for "living" field
    let b = 0.8 * avg_v;  // Strong blue-cyan tint
    
    // Opacity: faint but readable (0.08 to 0.22 range, scales with concentration)
    let alpha = (0.08 + 0.14 * avg_v).clamp(0.08, 0.22);
    
    // Full-world quad: center at world center, covers entire 256×256 chunk world
    let world_center = WORLD_EXTENT_PIXELS / 2.0;
    let world_size = WORLD_EXTENT_PIXELS;
    
    commands.spawn((
        VisualDebugGsFullWorldTint,
        Sprite::from_color(
            Color::srgba(r, g, b, alpha),
            Vec2::splat(world_size),
        ),
        Transform::from_translation(Vec3::new(world_center, world_center, 1.5)),
    ));
}

fn visual_debug_thermal_overlay_system(
    mut commands: Commands,
    visual: Res<VisualDebug>,
    thermal: Res<ThermalState>,
    mut cache: ResMut<VisualDebugThermalCache>,
    time: Res<Time>,
    camera_query: Query<&Transform, With<Camera2d>>,
    existing: Query<Entity, With<VisualDebugThermalSprite>>,
) {
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }
    if !visual.enabled || thermal.local_temperature_by_chunk.is_empty() {
        cache.prev_temp_by_chunk.clear();
        return;
    }

    let camera = match camera_query.single() {
        Ok(value) => value,
        Err(_) => return,
    };
    let cam = Vec2::new(camera.translation.x, camera.translation.y);

    let mut hotspots: Vec<(ChunkId, f32)> = thermal
        .local_temperature_by_chunk
        .iter()
        .map(|(chunk, temp)| (*chunk, (*temp - thermal.sink_temperature_k).max(0.0)))
        .collect();
    hotspots.sort_by(|a, b| {
        let ad = Vec2::new(a.0 .0 as f32 * CHUNK_PIXEL, a.0 .1 as f32 * CHUNK_PIXEL).distance_squared(cam);
        let bd = Vec2::new(b.0 .0 as f32 * CHUNK_PIXEL, b.0 .1 as f32 * CHUNK_PIXEL).distance_squared(cam);
        match ad.partial_cmp(&bd) {
            Some(ordering) => ordering,
            None => Ordering::Equal,
        }
    });
    let mut hottest: Vec<(ChunkId, f32)> = hotspots.into_iter().take(VISUAL_DEBUG_MAX_THERMAL_SPRITES * 3).collect();
    hottest.sort_by(|a, b| match b.1.partial_cmp(&a.1) {
        Some(ordering) => ordering,
        None => Ordering::Equal,
    });

    let denom = hottest
        .first()
        .map(|(_, delta)| *delta);
    let denom = match denom {
        Some(value) => value,
        None => 1.0,
    }
        .max(0.001);
    let time_s = time.elapsed_secs();
    
    // Global breathing pulse synced to overall thermal system activity (0.7 Hz)
    let global_breath = (time_s * 0.7 * std::f32::consts::TAU).sin();
    
    for (chunk, delta) in hottest.into_iter().take(VISUAL_DEBUG_MAX_THERMAL_SPRITES) {
        let intensity = (delta / denom).clamp(0.0, 1.0);
        let current_temp = thermal.local_temperature_by_chunk.get(&chunk).copied();
        let current_temp = match current_temp {
            Some(value) => value,
            None => thermal.sink_temperature_k,
        };
        let prev_temp = cache.prev_temp_by_chunk.get(&chunk).copied();
        let prev_temp = match prev_temp {
            Some(value) => value,
            None => current_temp,
        };
        let rate = (current_temp - prev_temp).abs().clamp(0.0, 2.0);
        let phase = chunk.0 as f32 * 0.03 + chunk.1 as f32 * 0.05;
        let local_pulse = (time_s * (2.8 + rate * 4.0) + phase).sin().abs();
        
        // Combine local pulse with global breathing for stronger effect
        let combined_pulse = (0.5 * local_pulse + 0.5 * (global_breath * 0.5 + 0.5)).clamp(0.0, 1.0);
        let pulse_boost = (rate * 0.3 * combined_pulse).clamp(0.0, 0.35);
        
        // Intensified opacity: stronger base + stronger breathing pulse
        let alpha = (0.40 + intensity * 0.40 + pulse_boost * 0.25).clamp(0.25, 0.95);
        
        // Intensified saturation: boost red/orange-red for thermal appearance
        let red = (0.20 + intensity * 0.95).clamp(0.2, 1.0);
        let green = (0.02 + pulse_boost * 0.30 + rate * 0.08).clamp(0.0, 0.5);
        let blue = (0.98 - intensity * 0.92).clamp(0.1, 1.0);
        
        let color = Color::srgba(red, green, blue, alpha);
        let pos = clip_to_world_bounds(chunk_to_translation(&chunk, 1.0));
        
        commands.spawn((
            VisualDebugThermalSprite,
            Sprite::from_color(color, Vec2::splat((CHUNK_PIXEL - 1.0 + pulse_boost * 6.0).max(1.0))),
            Transform::from_translation(pos),
        ));
        cache.prev_temp_by_chunk.insert(chunk, current_temp);
    }
}

fn visual_debug_hypergraph_overlay_system(
    visual: Res<VisualDebug>,
    keys: Res<ButtonInput<KeyCode>>,
    camera_query: Query<&Projection, With<Camera2d>>,
    substrate: Res<HypergraphSubstrate>,
    mut gizmos: Gizmos,
) {
    if !visual.enabled {
        return;
    }

    let zoomed_in = match camera_query.single() {
        Ok(Projection::Orthographic(ortho)) => ortho.scale <= 0.9,
        _ => false,
    };
    let show_detail = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) || zoomed_in;
    if !show_detail {
        return;
    }

    let mut shown = 0usize;
    for coord in substrate.patch_coords() {
        if shown >= 80 {
            break;
        }
        let output = match substrate.patch_output(coord) {
            Some(value) => value,
            None => continue,
        };
        let (chunk_x, chunk_y) = substrate.patch_primary_chunk(coord);
        let center = Vec2::new(
            chunk_x as f32 * CHUNK_PIXEL + CHUNK_PIXEL * 0.5,
            chunk_y as f32 * CHUNK_PIXEL + CHUNK_PIXEL * 0.5,
        );
        let glow = Color::srgba(
            (0.2 + output.clustering * 0.6).clamp(0.0, 1.0),
            (0.2 + output.usable_flux * 0.7).clamp(0.0, 1.0),
            (0.35 + output.causal_volume * 0.55).clamp(0.0, 1.0),
            0.25,
        );
        let radius = 1.6 + output.density * 3.2;
        gizmos.circle_2d(center, radius, glow);
        let ribbon = 3.0 + output.avg_arity * 4.0;
        gizmos.line_2d(center, center + Vec2::new(ribbon, ribbon * 0.35), glow);
        shown += 1;
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

#[cfg(test)]
mod tests {
    use super::{
        advance_simlife_grass, apply_heat_and_cooling_to_chunk, camera_viewport_bounds,
        food_portions_from_grass, force_panic_error_handlers,
        interactive_tier1_setting_from_flags, live_stats_panel_layout,
        run_headless_with_target_ticks, visible_world_bounds, world_bounds_pixels,
        HeadlessTermination, SimLifeState, ThermalState,
        HEADLESS_SURVIVAL_BASELINE_TICK, SIMLIFE_GRASS_MAX,
        THERMAL_HEAT_PER_USABLE_FLUX_CHEMISTRY, WORLD_EXTENT_PIXELS,
    };
    use bevy::app::{AppLabel, SubApp};
    use bevy::prelude::*;
    use simrard_lib_charter::ChunkId;
    use simrard_lib_hypergraph::HypergraphSubstrate;

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

    #[test]
    fn injected_flux_creates_hotspot_then_cools_toward_sink() {
        let mut substrate = HypergraphSubstrate::default();
        let mut thermal = ThermalState::default();
        let chunk = ChunkId(0, 0);

        substrate.inject_usable_flux_for_chunk(chunk.0, chunk.1, 0.8);
        let flux = substrate.consume_usable_flux_for_chunk(chunk.0, chunk.1, 0.5);
        apply_heat_and_cooling_to_chunk(
            &mut thermal,
            chunk,
            flux * THERMAL_HEAT_PER_USABLE_FLUX_CHEMISTRY,
        );

        let heated = thermal.temperature_for_chunk(chunk);
        assert!(heated > thermal.sink_temperature_k);

        for _ in 0..32 {
            apply_heat_and_cooling_to_chunk(&mut thermal, chunk, 0.0);
        }

        let cooled = thermal.temperature_for_chunk(chunk);
        assert!(cooled < heated);
        assert!(cooled <= thermal.sink_temperature_k + 0.25);
    }

    fn make_ortho_for_resolution(width: f32, height: f32, scale: f32) -> OrthographicProjection {
        let half_w = width * scale * 0.5;
        let half_h = height * scale * 0.5;
        let mut ortho = OrthographicProjection::default_2d();
        ortho.scale = scale;
        ortho.area = Rect::new(-half_w, -half_h, half_w, half_h);
        ortho
    }

    #[test]
    fn viewport_bounds_match_world_bounds_under_multiple_aspects() {
        let aspects = [
            (1024.0, 768.0),   // 4:3
            (1280.0, 720.0),   // 16:9
            (1280.0, 800.0),   // 16:10
            (3440.0, 1440.0),  // ultrawide
        ];
        let transform = Transform::from_xyz(WORLD_EXTENT_PIXELS * 0.5, WORLD_EXTENT_PIXELS * 0.5, 0.0);

        for (w, h) in aspects {
            let ortho = make_ortho_for_resolution(w, h, 10.0);
            let (vx0, vx1, vy0, vy1) = camera_viewport_bounds(&transform, &ortho);
            let vis = visible_world_bounds(vx0, vx1, vy0, vy1);
            assert!(vis.is_some(), "world must remain visible for aspect {}x{}", w, h);
            let (min_x, max_x, min_y, max_y) = match vis {
                Some(value) => value,
                None => unreachable!(),
            };

            let (world_min_x, world_max_x, world_min_y, world_max_y) = world_bounds_pixels();
            assert!(min_x >= world_min_x, "left clamp drift for aspect {}x{}", w, h);
            assert!(max_x <= world_max_x, "right clamp drift for aspect {}x{}", w, h);
            assert!(min_y >= world_min_y, "bottom clamp drift for aspect {}x{}", w, h);
            assert!(max_y <= world_max_y, "top clamp drift for aspect {}x{}", w, h);
        }
    }

    #[test]
    fn world_bounds_are_exact_256_by_256_chunks() {
        let (min_x, max_x, min_y, max_y) = world_bounds_pixels();
        assert_eq!(min_x, 0.0);
        assert_eq!(min_y, 0.0);
        assert_eq!(max_x, WORLD_EXTENT_PIXELS);
        assert_eq!(max_y, WORLD_EXTENT_PIXELS);
    }

    #[test]
    fn tier1_flag_defaults_to_visual_debug_only() {
        let setting = interactive_tier1_setting_from_flags(false, false);
        assert_eq!(setting, Ok(false));
    }

    #[test]
    fn tier1_flag_enables_interactive_spawning_when_requested() {
        let setting = interactive_tier1_setting_from_flags(false, true);
        assert_eq!(setting, Ok(true));
    }

    #[test]
    fn tier1_flag_conflict_is_rejected() {
        let setting = interactive_tier1_setting_from_flags(true, true);
        assert!(setting.is_err());
    }

    #[test]
    fn resolution_agnostic_live_resize_keeps_square_world_and_panel_anchor() {
        // Simulate a live-resize sequence (narrow, wide, tall, square-ish).
        let sizes = [
            (1000.0, 700.0),
            (1400.0, 700.0),
            (900.0, 900.0),
            (1600.0, 900.0),
            (800.0, 1100.0),
        ];

        let transform = Transform::from_xyz(WORLD_EXTENT_PIXELS * 0.5, WORLD_EXTENT_PIXELS * 0.5, 0.0);
        let (world_min_x, world_max_x, world_min_y, world_max_y) = world_bounds_pixels();

        for (w, h) in sizes {
            let ortho = make_ortho_for_resolution(w, h, 10.0);
            let (vx0, vx1, vy0, vy1) = camera_viewport_bounds(&transform, &ortho);
            let vis = visible_world_bounds(vx0, vx1, vy0, vy1);
            assert!(vis.is_some(), "world not visible during resize at {}x{}", w, h);

            let (panel_cx, panel_cy, _tx, _ty, panel_w, panel_h) =
                live_stats_panel_layout(transform.translation.x, transform.translation.y, ortho.area.max.x, ortho.area.max.y);
            let panel_left = panel_cx - panel_w * 0.5;
            let panel_right = panel_cx + panel_w * 0.5;
            let panel_bottom = panel_cy - panel_h * 0.5;
            let panel_top = panel_cy + panel_h * 0.5;

            // Panel should remain anchored within the camera view top-right area.
            assert!(panel_right <= vx1 + 0.001, "panel exceeds right viewport edge at {}x{}", w, h);
            assert!(panel_top <= vy1 + 0.001, "panel exceeds top viewport edge at {}x{}", w, h);
            assert!(panel_left >= vx0 - 0.001, "panel exceeds left viewport edge at {}x{}", w, h);
            assert!(panel_bottom >= vy0 - 0.001, "panel exceeds bottom viewport edge at {}x{}", w, h);

            // World bounds must remain invariant regardless of resize.
            assert_eq!(world_min_x, 0.0);
            assert_eq!(world_min_y, 0.0);
            assert_eq!(world_max_x, WORLD_EXTENT_PIXELS);
            assert_eq!(world_max_y, WORLD_EXTENT_PIXELS);
        }
    }
}

/// Pixels per chunk for 2D display. Chunk (0,0) at origin; (10,10) at (400, 400).
const CHUNK_PIXEL: f32 = 40.0;
const WORLD_EXTENT_PIXELS: f32 = 256.0 * CHUNK_PIXEL; // 10240.0

/// Clip a world position to the valid world bounds [0, WORLD_EXTENT_PIXELS]²
fn clip_to_world_bounds(pos: Vec3) -> Vec3 {
    Vec3::new(
        pos.x.clamp(0.0, WORLD_EXTENT_PIXELS),
        pos.y.clamp(0.0, WORLD_EXTENT_PIXELS),
        pos.z,
    )
}

/// Food = large orange so clearly distinct from water and pawns.
const SPRITE_FOOD: f32 = 18.0;
/// Water = medium cyan so clearly distinct from blue/purple thirst pawns.
const SPRITE_WATER: f32 = 14.0;
const SPRITE_PAWN: f32 = 10.0;
const RESOURCE_BAR_HEIGHT: f32 = 3.0;
const RESOURCE_BAR_MAX_WIDTH: f32 = 18.0;
const RESOURCE_BAR_Y_OFFSET: f32 = 13.0;
const RESOURCE_BAR_MAX_PORTIONS: f32 = 8.0;
const CAMERA_PAN_SPEED: f32 = 4000.0;
const CAMERA_MIN_ZOOM: f32 = 0.25;
const CAMERA_MAX_ZOOM: f32 = 20.0;
const CAMERA_ZOOM_STEP: f32 = 0.12;
// Initial camera scale to show the full 256x256 world (~10240 px) in window
const CAMERA_INITIAL_SCALE: f32 = 10.0;
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

fn setup(
    mut commands: Commands,
    mut allocator: ResMut<ItemIdAllocator>,
    sim_mode: Option<Res<SimulationModeRuntime>>,
) {
    // Center camera on world and zoom to show full 256x256 extent
    let world_cx = CHUNK_EXTENT as f32 * CHUNK_PIXEL / 2.0;
    let world_cy = CHUNK_EXTENT as f32 * CHUNK_PIXEL / 2.0;
    commands.spawn((
        Camera2d,
        Transform::from_xyz(world_cx, world_cy, 999.9),
        Projection::Orthographic(OrthographicProjection {
            scale: CAMERA_INITIAL_SCALE,
            ..OrthographicProjection::default_2d()
        }),
    ));

    // Axis coordinate labels every 32 chunks - smaller font, positioned outside world bounds
    let axis_font = TextFont { font_size: 96.0, ..default() };
    let axis_color = TextColor(Color::srgba(0.72, 0.78, 0.92, 0.72));
    let label_margin = -480.0;
    for ci in (0u32..=256).step_by(32) {
        let wx = ci as f32 * CHUNK_PIXEL;
        // X-axis labels below grid
        commands.spawn((
            Text2d::new(format!("{}", ci)),
            axis_font.clone(),
            axis_color,
            Transform::from_translation(Vec3::new(wx, label_margin, 5.0)),
        ));
        // Y-axis labels left of grid
        commands.spawn((
            Text2d::new(format!("{}", ci)),
            axis_font.clone(),
            axis_color,
            Transform::from_translation(Vec3::new(label_margin, wx, 5.0)),
        ));
    }

    let tier1_enabled = match sim_mode {
        Some(mode) => match mode.mode {
            SimulationMode::Interactive => mode.interactive_with_tier1,
            SimulationMode::Headless => mode.interactive_with_tier1,
        },
        None => false,
    };
    if !tier1_enabled {
        return;
    }

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
struct ThermalFieldWrite;

// Gray-Scott reaction-diffusion constants (Tier 5/6 biomass field).
// Spot-forming parameter regime: F=0.055, k=0.062.
const GS_DU: f32 = 0.16;
const GS_DV: f32 = 0.08;
const GS_F_BASE: f32 = 0.055;
const GS_K: f32 = 0.062;
const GS_DT: f32 = 1.0;
/// GS runs every this many sim ticks; at target 1000 Hz → 200 Hz GS rate.
const GS_UPDATE_INTERVAL_TICKS: u64 = 5;
/// Substrate-only mode runs GS at lower cadence to prioritize benchmark throughput.
const GS_UPDATE_INTERVAL_TICKS_SUBSTRATE: u64 = 200;
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
/// Chemistry hotspots softly bias local GS feed toward active nutrient regions.
const GS_F_CHEMISTRY_HOTSPOT_SCALE: f32 = 0.03;
/// Conserved usable flux from Tier 10 modestly raises local GS feed where energy is available.
const GS_F_USABLE_FLUX_SCALE: f32 = 0.018;
/// Substrate mode default initial GS seeded density (~0.1% for sparse activation).
const GS_INITIAL_SEED_COVERAGE: f32 = 0.001;
/// Tier 10 activation tuning for substrate profile.
const SUBSTRATE_HYPERGRAPH_INTERVAL_TICKS: u64 = 800;
const SUBSTRATE_HYPERGRAPH_INTERVAL_TICKS_PRE_TUNE: u64 = 1_000;
const SUBSTRATE_HYPERGRAPH_CHAOS: f32 = 0.45;
const VISUAL_DEBUG_HYPERGRAPH_INTERVAL_TICKS: u64 = 1;
const T9_SINK_TEMPERATURE_K: f32 = 2.7;
const T9_COOLING_K: f32 = 0.08;
const THERMAL_HEAT_PER_USABLE_FLUX_CHEMISTRY: f32 = 3.0;
const THERMAL_HEAT_PER_USABLE_FLUX_SIMLIFE: f32 = 1.8;
const THERMAL_HEAT_PER_PLANT_DECOMP: f32 = 0.45;
const THERMAL_CHEMISTRY_FLUX_DRAW: f32 = 0.035;
const THERMAL_SIMLIFE_FLUX_DRAW_BASE: f32 = 0.012;
const THERMAL_SIMLIFE_ACTIVITY_HEAT_SCALE: f32 = 0.08;
const SIMLIFE_DECOMP_TO_CHEMISTRY_SCALE: f32 = 0.035;
const THERMAL_RUNAWAY_BOIL_K: f32 = 24.0;
const THERMAL_RUNAWAY_FREEZE_K: f32 = 2.71;

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

#[derive(Resource, Debug, Clone)]
struct ThermalState {
    local_temperature_by_chunk: HashMap<ChunkId, f32>,
    sink_temperature_k: f32,
    cooling_k: f32,
    cumulative_flux_consumed: f64,
    cumulative_heat_dissipated: f64,
    peak_temperature_seen_k: f32,
    latest_peak_temperature_k: f32,
}

impl Default for ThermalState {
    fn default() -> Self {
        Self {
            local_temperature_by_chunk: HashMap::new(),
            sink_temperature_k: T9_SINK_TEMPERATURE_K,
            cooling_k: T9_COOLING_K,
            cumulative_flux_consumed: 0.0,
            cumulative_heat_dissipated: 0.0,
            peak_temperature_seen_k: T9_SINK_TEMPERATURE_K,
            latest_peak_temperature_k: T9_SINK_TEMPERATURE_K,
        }
    }
}

impl ThermalState {
    fn temperature_for_chunk(&self, chunk: ChunkId) -> f32 {
        match self.local_temperature_by_chunk.get(&chunk).copied() {
            Some(value) => value,
            None => self.sink_temperature_k,
        }
    }
}

fn apply_heat_and_cooling_to_chunk(thermal: &mut ThermalState, chunk: ChunkId, heat_gain_k: f32) {
    let current = thermal.temperature_for_chunk(chunk);
    let heated = current + heat_gain_k.max(0.0);
    let cooled = thermal.sink_temperature_k
        + (heated - thermal.sink_temperature_k) * (1.0 - thermal.cooling_k).clamp(0.0, 1.0);
    let dissipated = (heated - cooled).max(0.0);
    thermal.cumulative_heat_dissipated += dissipated as f64;
    thermal.peak_temperature_seen_k = thermal.peak_temperature_seen_k.max(cooled);
    if cooled <= thermal.sink_temperature_k + 0.001 {
        thermal.local_temperature_by_chunk.remove(&chunk);
    } else {
        thermal.local_temperature_by_chunk.insert(chunk, cooled);
    }
}

fn note_latest_peak_temperature(thermal: &mut ThermalState) {
    let peak = thermal
        .local_temperature_by_chunk
        .values()
        .copied()
        .fold(thermal.sink_temperature_k, f32::max);
    thermal.latest_peak_temperature_k = peak;
    thermal.peak_temperature_seen_k = thermal.peak_temperature_seen_k.max(peak);
}

fn thermal_passive_cooling_system(global_clock: Res<GlobalTickClock>, mut thermal: ResMut<ThermalState>) {
    let seq = global_clock.causal_seq();
    if seq % 1000 != 0 {
        return;
    }
    if thermal.local_temperature_by_chunk.is_empty() {
        return;
    }
    let chunks: Vec<ChunkId> = thermal.local_temperature_by_chunk.keys().copied().collect();
    for chunk in chunks {
        apply_heat_and_cooling_to_chunk(&mut thermal, chunk, 0.0);
    }
    note_latest_peak_temperature(&mut thermal);
}

fn gs_temperature_growth_modifier(local_temp_k: f32) -> f32 {
    if local_temp_k >= 18.0 {
        0.55
    } else if local_temp_k >= 12.0 {
        0.75
    } else if local_temp_k <= T9_SINK_TEMPERATURE_K + 0.25 {
        0.92
    } else {
        1.0
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
    advance_simlife_grass_with_hypergraph(current_seq, simlife, None, None, None, &mut charter, None);
}

/// Advance the Gray-Scott Tier 5/6 field by one logical tick.
/// Seeding happens automatically on first call. GS update runs every GS_UPDATE_INTERVAL_TICKS.
fn advance_simlife_grass_with_hypergraph(
    current_seq: u64,
    simlife: &mut SimLifeState,
    hypergraph: Option<&mut HypergraphSubstrate>,
    chemistry: Option<&mut ChemistryState>,
    thermal: Option<&mut ThermalState>,
    charter: &mut SpatialCharter,
    report: Option<&mut SimulationReport>,
) {
    if current_seq <= simlife.last_tick {
        return;
    }
    simlife.last_tick = current_seq;

    // Seed initial state when all active cells have been exhausted (or on cold start).
    if simlife.gs_active.is_empty() {
        gs_seed_initial_state(current_seq, simlife, None, charter);
    }

    // Throttle: only run the GS stencil at mode-specific interval.
    if current_seq.saturating_sub(simlife.last_gs_tick) < gs_update_interval_ticks_for_mode() {
        return;
    }
    simlife.last_gs_tick = current_seq;

    gs_update(current_seq, simlife, hypergraph, chemistry, thermal, charter, report);
}

fn substrate_hotspot_centers() -> [(u32, u32); 8] {
    [
        (32, 32),
        (32, CHUNK_EXTENT.saturating_sub(32)),
        (CHUNK_EXTENT.saturating_sub(32), 32),
        (CHUNK_EXTENT.saturating_sub(32), CHUNK_EXTENT.saturating_sub(32)),
        (CHUNK_EXTENT / 2, CHUNK_EXTENT / 2),
        (CHUNK_EXTENT / 2, 40),
        (40, CHUNK_EXTENT / 2),
        (CHUNK_EXTENT.saturating_sub(40), CHUNK_EXTENT / 2),
    ]
}

fn substrate_seed_coverage() -> f32 {
    if ISOLATION_MINIMAL_SEEDING {
        0.001
    } else {
        GS_INITIAL_SEED_COVERAGE
    }
}

fn substrate_seed_centers() -> Vec<(u32, u32)> {
    let centers = substrate_hotspot_centers();
    if ISOLATION_MINIMAL_SEEDING {
        centers.to_vec()
    } else {
        centers[..4].to_vec()
    }
}

fn gs_update_interval_ticks_for_mode() -> u64 {
    if headless_substrate_from_args() {
        GS_UPDATE_INTERVAL_TICKS_SUBSTRATE
    } else {
        GS_UPDATE_INTERVAL_TICKS
    }
}

fn seeded_uv_for_chunk(chunk: ChunkId) -> (f32, f32) {
    let hash = ((chunk.0 as u64) << 32) ^ (chunk.1 as u64) ^ 0x9e37_79b9;
    let u = 0.2 + ((hash & 0xff) as f32 / 255.0) * 0.4;
    let v = 0.2 + (((hash >> 8) & 0xff) as f32 / 255.0) * 0.4;
    (u.clamp(0.2, 0.6), v.clamp(0.2, 0.6))
}

/// Seed sparse GS + chemistry hotspots through charter leases.
/// Returns number of seeded GS cells.
fn gs_seed_initial_state(
    current_seq: u64,
    simlife: &mut SimLifeState,
    chemistry: Option<&mut ChemistryState>,
    charter: &mut SpatialCharter,
) -> usize {
    let total_cells = ((CHUNK_EXTENT as usize) + 1).pow(2);
    let target_seed_cells = (total_cells as f32 * substrate_seed_coverage()) as usize;

    let mut seeded_cells = 0_usize;
    let centers = substrate_seed_centers();

    if let Some(chemistry) = chemistry {
        for (cx, cy) in &centers {
            for dx in -6_i32..=6_i32 {
                for dy in -6_i32..=6_i32 {
                    let x = *cx as i32 + dx;
                    let y = *cy as i32 + dy;
                    if x < 0 || y < 0 || x > CHUNK_EXTENT as i32 || y > CHUNK_EXTENT as i32 {
                        continue;
                    }
                    let dist2 = (dx * dx + dy * dy) as f32;
                    if dist2 > 36.0 {
                        continue;
                    }
                    let chunk = ChunkId(x as u32, y as u32);
                    let intensity = ((36.0 - dist2) / 36.0).clamp(0.0, 1.0);
                    let noise_floor = (0.08 + intensity * 0.24).clamp(0.0, HYPERGRAPH_NOISE_FLOOR_MAX);
                    let lease_req = SpatialLease {
                        primary: chunk,
                        fringe: vec![],
                        intent: LeaseIntent {
                            reads: vec![],
                            writes: vec![TypeId::of::<ChemistryNoiseFloorWrite>()],
                        },
                        granted_at_causal_seq: current_seq,
                    };
                    if let Ok(handle) = charter.request_lease(lease_req, current_seq) {
                        chemistry.receptor_noise_floor_by_chunk.insert(chunk, noise_floor);
                        charter.release_lease(handle);
                    }
                }
            }
        }
    }

    'seed: for (cx, cy) in &centers {
        for dx in -5_i32..=5_i32 {
            for dy in -5_i32..=5_i32 {
                let x = *cx as i32 + dx;
                let y = *cy as i32 + dy;
                if x < 0 || y < 0 || x > CHUNK_EXTENT as i32 || y > CHUNK_EXTENT as i32 {
                    continue;
                }
                let dist2 = (dx * dx + dy * dy) as f32;
                if dist2 > 25.0 {
                    continue;
                }
                let chunk = ChunkId(x as u32, y as u32);
                if simlife.v_field.contains_key(&chunk) {
                    continue;
                }
                let lease_req = SpatialLease {
                    primary: chunk,
                    fringe: vec![],
                    intent: LeaseIntent {
                        reads: vec![],
                        writes: vec![TypeId::of::<SimLifeGrayScottWrite>()],
                    },
                    granted_at_causal_seq: current_seq,
                };
                if let Ok(handle) = charter.request_lease(lease_req, current_seq) {
                    let (u, v) = seeded_uv_for_chunk(chunk);
                    simlife.u_field.insert(chunk, u);
                    simlife.v_field.insert(chunk, v);
                    let grass = (v * SIMLIFE_GRASS_MAX as f32) as u32;
                    if grass > 0 {
                        simlife.grass_per_chunk.insert(chunk, grass);
                    }
                    simlife.gs_active.insert(chunk);
                    for (nx, ny) in gs_neighbor_coords(chunk.0, chunk.1) {
                        simlife.gs_active.insert(ChunkId(nx, ny));
                    }
                    charter.release_lease(handle);
                    seeded_cells += 1;
                }
                if seeded_cells >= target_seed_cells {
                    break 'seed;
                }
            }
        }
    }

    seeded_cells
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

fn output_usable_flux_for_cell(hypergraph: Option<&HypergraphSubstrate>, cx: u32, cy: u32) -> f32 {
    match hypergraph.and_then(|hg| hg.output_for_chunk(cx, cy)) {
        Some(output) => output.usable_flux,
        None => 0.0,
    }
}

/// One Gray-Scott update step over the sparse active frontier.
/// Read phase computes all delta-U/V from the CURRENT field (no in-place hazard).
/// Write phase applies updates via charter leases; denied cells preserve their old value.
fn gs_update(
    current_seq: u64,
    simlife: &mut SimLifeState,
    mut hypergraph: Option<&mut HypergraphSubstrate>,
    mut chemistry: Option<&mut ChemistryState>,
    mut thermal: Option<&mut ThermalState>,
    charter: &mut SpatialCharter,
    mut report: Option<&mut SimulationReport>,
) {
    let active_cells: Vec<ChunkId> = simlife.gs_active.iter().copied().collect();

    // ── READ PHASE ─────────────────────────────────────────────────────────
    // Compute new (u, v) for every active cell from the *current* (unmodified) field.
    let hypergraph_read = hypergraph.as_deref();
    let thermal_read = thermal.as_deref();
    let chemistry_read = chemistry.as_deref();
    let mut updates: Vec<(ChunkId, f32, f32, f32, f32, f32)> = Vec::with_capacity(active_cells.len());
    for &cell in &active_cells {
        let ChunkId(cx, cy) = cell;
        let u = *simlife.u_field.get(&cell).unwrap_or(&1.0); // GS_SPARSE_FIELD_DEFAULT
        let v = *simlife.v_field.get(&cell).unwrap_or(&0.0); // GS_SPARSE_FIELD_DEFAULT
        let local_temp_k = match thermal_read {
            Some(state) => state.temperature_for_chunk(cell),
            None => T9_SINK_TEMPERATURE_K,
        };
        let thermal_modifier = gs_temperature_growth_modifier(local_temp_k);

        // Local feed rate: base + softened hypergraph clustering/causal-volume modulation.
        let f_local = if let Some(hg) = hypergraph_read {
            if let Some(output) = hg.output_for_chunk(cx, cy) {
                let clustering = output.clustering.clamp(0.0, 1.0)
                    * GS_F_CLUSTERING_SCALE
                    * GS_CLUSTERING_MULTIPLIER;
                let causal_volume = output.causal_volume.clamp(0.0, 1.0)
                    * GS_F_CAUSAL_VOLUME_SCALE
                    * GS_CAUSAL_VOLUME_MULTIPLIER;
                let flux_bias = output.usable_flux.clamp(0.0, 1.0) * GS_F_USABLE_FLUX_SCALE;
                let chemistry_hotspot_raw = chemistry_read
                    .and_then(|c| c.receptor_noise_floor_by_chunk.get(&cell).copied());
                let chemistry_hotspot = match chemistry_hotspot_raw {
                    Some(value) => value,
                    None => 0.0,
                } / HYPERGRAPH_NOISE_FLOOR_MAX;
                let hotspot_bias = chemistry_hotspot.clamp(0.0, 1.0) * GS_F_CHEMISTRY_HOTSPOT_SCALE;
                ((GS_F_BASE + clustering + causal_volume + hotspot_bias + flux_bias) * thermal_modifier).clamp(0.01, 0.12)
            } else {
                (GS_F_BASE * thermal_modifier).clamp(0.01, 0.12)
            }
        } else {
            (GS_F_BASE * thermal_modifier).clamp(0.01, 0.12)
        };

        let lap_u = gs_laplacian(cx, cy, &simlife.u_field, 1.0);
        let lap_v = gs_laplacian(cx, cy, &simlife.v_field, 0.0);

        let uvv = u * v * v;
        let new_u = (u + GS_DT * (GS_DU * lap_u - uvv + f_local * (1.0 - u))).clamp(0.0, 1.0);
        let new_v = (v + GS_DT * (GS_DV * lap_v + uvv - (f_local + GS_K) * v)).clamp(0.0, 1.0);
        let flux_request = THERMAL_SIMLIFE_FLUX_DRAW_BASE + output_usable_flux_for_cell(hypergraph_read, cx, cy) * 0.05;
        let activity_heat = (uvv.abs() + new_v) * THERMAL_SIMLIFE_ACTIVITY_HEAT_SCALE;

        updates.push((cell, new_u, new_v, v, flux_request, activity_heat));
    }

    // ── WRITE PHASE ────────────────────────────────────────────────────────
    // Apply updates through charter leases. Build next active frontier.
    let mut new_active: HashSet<ChunkId> = HashSet::new();
    let mut lease_handles: Vec<LeaseHandle> = Vec::new();
    let mut lease_grants = 0_u64;
    let mut lease_denials = 0_u64;

    for (cell, new_u, new_v, old_v, flux_request, activity_heat) in updates {
        let ChunkId(cx, cy) = cell;
        let lease_req = SpatialLease {
            primary: cell,
            fringe: vec![],
            intent: LeaseIntent {
                reads: vec![],
                writes: vec![
                    TypeId::of::<SimLifeGrayScottWrite>(),
                    TypeId::of::<ChemistryNoiseFloorWrite>(),
                    TypeId::of::<ThermalFieldWrite>(),
                    TypeId::of::<HypergraphRegionalOutputWrite>(),
                ],
            },
            granted_at_causal_seq: current_seq,
        };
        match charter.request_lease(lease_req, current_seq) {
            Ok(handle) => {
                lease_grants += 1;
                let mut flux_draw = 0.0_f32;
                if let Some(hg) = hypergraph.as_deref_mut() {
                    flux_draw = hg.consume_usable_flux_for_chunk(cx, cy, flux_request);
                }
                simlife.u_field.insert(cell, new_u);
                simlife.v_field.insert(cell, new_v);
                let plant_decay = (old_v - new_v).max(0.0);
                if let Some(chemistry_state) = chemistry.as_deref_mut() {
                    let existing_noise = chemistry_state
                        .receptor_noise_floor_by_chunk
                        .get(&cell)
                        .copied();
                    let existing_noise = match existing_noise {
                        Some(value) => value,
                        None => 0.0,
                    };
                    let deposit = (plant_decay * SIMLIFE_DECOMP_TO_CHEMISTRY_SCALE).max(0.0);
                    let next_noise = (existing_noise + deposit).min(HYPERGRAPH_NOISE_FLOOR_MAX);
                    chemistry_state
                        .receptor_noise_floor_by_chunk
                        .insert(cell, next_noise);
                }
                if let Some(thermal_state) = thermal.as_deref_mut() {
                    thermal_state.cumulative_flux_consumed += flux_draw as f64;
                    apply_heat_and_cooling_to_chunk(
                        thermal_state,
                        cell,
                        activity_heat
                            + flux_draw * THERMAL_HEAT_PER_USABLE_FLUX_SIMLIFE
                            + plant_decay * THERMAL_HEAT_PER_PLANT_DECOMP,
                    );
                }
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
                lease_denials += 1;
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

    if let Some(ref mut report) = report {
        if !ISOLATION_DISABLE_T60_AND_EXTRA_COUNTERS {
            for _ in 0..lease_grants {
                report.bump("simlife_lease_grants");
            }
            for _ in 0..lease_denials {
                report.bump("simlife_lease_denials");
            }
        }
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
    mut hypergraph: ResMut<HypergraphSubstrate>,
    mut chemistry: ResMut<ChemistryState>,
    mut thermal: ResMut<ThermalState>,
    mut charter: ResMut<SpatialCharter>,
    mut report: Option<ResMut<SimulationReport>>,
) {
    let started = Instant::now();
    // Causal ordering: run after sim_tick_driver, then respawn reads same-tick SimLife state.
    if tier10_enabled_from_args() {
        advance_simlife_grass_with_hypergraph(
            global_clock.causal_seq(),
            &mut simlife,
            Some(&mut hypergraph),
            Some(&mut chemistry),
            Some(&mut thermal),
            &mut charter,
            report.as_deref_mut(),
        );
    } else {
        advance_simlife_grass_with_hypergraph(
            global_clock.causal_seq(),
            &mut simlife,
            None,
            None,
            Some(&mut thermal),
            &mut charter,
            report.as_deref_mut(),
        );
    }
    perf_record("simlife_tick", started.elapsed());
}

fn run_hypergraph_tick_preupdate(sim_mode: Res<SimulationModeRuntime>) -> bool {
    sim_mode.mode != SimulationMode::Interactive || sim_mode.interactive_with_tier1
}

fn run_hypergraph_tick_visual_debug_update(
    visual: Res<VisualDebug>,
    sim_mode: Res<SimulationModeRuntime>,
) -> bool {
    visual.enabled && sim_mode.mode == SimulationMode::Interactive && !sim_mode.interactive_with_tier1
}

fn activate_visual_debug_hypergraph_system(
    mut simlife: ResMut<SimLifeState>,
    mut chemistry: ResMut<ChemistryState>,
    mut charter: ResMut<SpatialCharter>,
    mut substrate: ResMut<HypergraphSubstrate>,
    mut runtime_stats: ResMut<HypergraphRuntimeStats>,
    mut activated: Local<bool>,
) {
    if *activated {
        return;
    }

    // Reuse the substrate-only activation path so visual-debug mode shows live rewrites quickly.
    substrate.set_interval_ticks(VISUAL_DEBUG_HYPERGRAPH_INTERVAL_TICKS);
    substrate.set_chaos(SUBSTRATE_HYPERGRAPH_CHAOS);
    let _ = gs_seed_initial_state(0, &mut simlife, Some(&mut chemistry), &mut charter);
    runtime_stats.rewritten_total = 0;
    *activated = true;
}

fn hypergraph_tick_system(
    global_clock: Res<GlobalTickClock>,
    mut charter: ResMut<SpatialCharter>,
    mut substrate: ResMut<HypergraphSubstrate>,
    mut chemistry: ResMut<ChemistryState>,
    mut thermal: ResMut<ThermalState>,
    mut runtime_stats: ResMut<HypergraphRuntimeStats>,
    mut report: Option<ResMut<SimulationReport>>,
    time: Res<Time>,
    mut audit_log: ResMut<HypergraphAuditLog>,
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
                    TypeId::of::<ThermalFieldWrite>(),
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
    if stats.considered > 0 {
        let coords: Vec<_> = substrate.patch_coords().collect();
        for coord in coords {
            let (chunk_x, chunk_y) = substrate.patch_primary_chunk(coord);
            if let Some(output) = substrate.patch_output(coord) {
                let noise_floor = (output.clustering.clamp(0.0, 1.0) * HYPERGRAPH_NOISE_FLOOR_MULTIPLIER)
                    .clamp(0.0, HYPERGRAPH_NOISE_FLOOR_MAX);
                let flux_draw = substrate.consume_usable_flux_for_patch(coord, THERMAL_CHEMISTRY_FLUX_DRAW);
                thermal.cumulative_flux_consumed += flux_draw as f64;
                apply_heat_and_cooling_to_chunk(
                    &mut thermal,
                    ChunkId(chunk_x, chunk_y),
                    flux_draw * THERMAL_HEAT_PER_USABLE_FLUX_CHEMISTRY,
                );
                chemistry
                    .receptor_noise_floor_by_chunk
                    .insert(ChunkId(chunk_x, chunk_y), noise_floor);
            }
        }
        note_latest_peak_temperature(&mut thermal);
    }
    perf_record("chemistry_noise_floor_hook", chemistry_started.elapsed());

    let release_started = Instant::now();
    for handle in active_handles {
        charter.release_lease(handle);
    }
    perf_record("charter_lease_release", release_started.elapsed());

    runtime_stats.rewritten_total = runtime_stats
        .rewritten_total
        .saturating_add(stats.rewritten as u64);

    if stats.rewritten > 0 {
        let entry = HypergraphAuditEntry {
            tick: seq,
            elapsed_secs: time.elapsed_secs(),
            rewrite_count: stats.rewritten as u64,
            description: format!("{} rewrites", stats.rewritten),
        };
        audit_log.push(entry);
    }

    if let Some(ref mut report) = report {
        if stats.considered > 0 {
            report.bump("hypergraph_ticks");
        }
        if !ISOLATION_DISABLE_T60_AND_EXTRA_COUNTERS {
            for _ in 0..stats.considered.saturating_sub(stats.denied) {
                report.bump("hypergraph_lease_grants");
            }
        }
        for _ in 0..stats.rewritten {
            report.bump("hypergraph_rewrites");
            if !ISOLATION_DISABLE_T60_AND_EXTRA_COUNTERS {
                report.bump("hypergraph_rule_fires");
            }
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
    if keys.just_pressed(KeyCode::KeyH) {
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
