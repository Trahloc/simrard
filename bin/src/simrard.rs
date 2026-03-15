use bevy::prelude::*;
use bevy::ecs::message::{MessageReader, Messages};
use bevy::input::mouse::MouseWheel;
use bevy::sprite::Sprite;
use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;
use simrard_lib_utility_ai::{BigBrainPlugin, BigBrainSet};
use simrard_lib_ai::{self as ai, build_pawn_brain, ActivityLog, PawnAIPlugin};
use simrard_lib_causal::{
    heartbeat, chebyshev_distance, propagation_delay, CausalEventKind, CausalEventQueue,
    CausalPlugin,
};
use simrard_lib_charter::{
    charter_watchguard_system, CharterFlashEvent, ChunkId, FrameWriteLog, SpatialCharter,
};
use simrard_lib_pawn::{
    Capabilities, FoodReservation, ItemHistory, ItemIdAllocator, ItemIdentity, KnownRecipes,
    NeuralNetworkComponent, Position, Quest, QuestBoard, QuestStatus, RestSpot,
    SimulationLogSettings, SimulationReport, WaterSource, WORLD_CHUNK_EXTENT,
};
use simrard_lib_time::{
    CausalClock, GlobalTickClock, SimTickAccumulator, SimTimeScale, TimePlugin,
    SIM_TICKS_PER_SECOND_AT_1X,
};
use simrard_lib_transforms::TransformsPlugin;
use simrard_lib_mirror::{push_ecs_snapshot_system, MirrorPlugin};

const HEADLESS_TARGET_TICKS: u64 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SimulationMode {
    Interactive,
    Headless,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HeadlessTermination {
    TickLimitReached,
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
            let result = run_headless();
            println!("{}", result.report);
            if matches!(result.termination, HeadlessTermination::Panic(_)) {
                std::process::exit(1);
            }
        }
    }
}

fn parse_mode() -> SimulationMode {
    if std::env::args().skip(1).any(|arg| arg == "--headless-test") {
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
        .init_resource::<RespawnState>()
        .init_resource::<SimLifeState>()
        .add_plugins(MirrorPlugin);
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
                ui_panel_update_system,
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

fn run_headless_with_target_ticks(target_ticks: u64) -> HeadlessRunResult {
    let mut app = App::new();
    app.set_error_handler(bevy::ecs::error::panic);
    // MinimalPlugins ensures Main/PreUpdate/Update run; without it PreUpdate may not run and pawns never eat/drink.
    app.add_plugins(MinimalPlugins)
        .add_plugins(BigBrainPlugin::new(PreUpdate))
        .add_plugins(PawnAIPlugin)
        .add_plugins(TimePlugin)
        .add_plugins(CausalPlugin)
        .init_resource::<QuestBoard>()
        .init_resource::<ItemIdAllocator>()
        .init_resource::<SpatialCharter>()
        .init_resource::<FrameWriteLog>()
        .init_resource::<Messages<CharterFlashEvent>>()
        .init_resource::<ActivityLog>()
        .init_resource::<RespawnState>()
        .init_resource::<SimLifeState>()
        .init_resource::<SimulationReport>()
        .insert_resource(SimulationLogSettings { stdout_enabled: false })
        .add_plugins(MirrorPlugin)
        .add_systems(Startup, (setup, initialize_report_baseline).chain())
        .add_systems(
            PreUpdate,
            (
                ai::pawn_death_system,
                ApplyDeferred,
                headless_tick_driver,
                simlife_tick_system,
                curiosity_discovery_system,
                resource_respawn_system,
            )
                .chain()
                .before(BigBrainSet::Scorers),
        )
        // Phase 4.D1: Push ECS snapshot to DuckDB mirror after each headless update.
        .add_systems(Update, push_ecs_snapshot_system);
    force_panic_error_handlers(&mut app);

    let started = Instant::now();
    let termination = loop {
        let update_result = catch_unwind(AssertUnwindSafe(|| app.update()));
        match update_result {
            Ok(()) => {
                let tick = app.world().resource::<GlobalTickClock>().causal_seq();
                if tick >= target_ticks {
                    break HeadlessTermination::TickLimitReached;
                }
                if count_living_pawns(app.world_mut()) == 0 {
                    break HeadlessTermination::AllPawnsDied;
                }
            }
            Err(payload) => {
                break HeadlessTermination::Panic(panic_payload_to_string(payload));
            }
        }
    };

    let report = build_headless_report(&mut app, &termination, started.elapsed().as_secs_f64());
    HeadlessRunResult { termination, report }
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

fn advance_simulation_one_tick(
    global_clock: &mut GlobalTickClock,
    event_queue: &mut CausalEventQueue,
    quest_board: &mut QuestBoard,
    activity: &mut ActivityLog,
    pawn_query: &mut Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
    capabilities_query: &Query<&Capabilities>,
    known_recipes_query: &mut Query<&mut KnownRecipes>,
    report: Option<&mut SimulationReport>,
    stdout_enabled: bool,
) {
    global_clock.increment();
    let seq = global_clock.causal_seq();
    let mut report = report;
    heartbeat::drive_decay_heartbeat_pulse(
        seq,
        pawn_query,
        event_queue,
        report.as_deref_mut(),
        stdout_enabled,
    );
    ai::pawn_event_dispatcher_step(
        seq,
        event_queue,
        quest_board,
        pawn_query,
        capabilities_query,
        known_recipes_query,
        Some(activity),
        report.as_deref_mut(),
        stdout_enabled,
    );
    if let Some(report) = report.as_deref_mut() {
        report.bump("sim_ticks_advanced");
    }
}

fn headless_tick_driver(
    mut global_clock: ResMut<GlobalTickClock>,
    mut event_queue: ResMut<CausalEventQueue>,
    mut quest_board: ResMut<QuestBoard>,
    mut activity: ResMut<ActivityLog>,
    mut pawn_query: Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
    capabilities_query: Query<&Capabilities>,
    mut known_recipes_query: Query<&mut KnownRecipes>,
    mut report: Option<ResMut<SimulationReport>>,
    log_settings: Option<Res<SimulationLogSettings>>,
) {
    advance_simulation_one_tick(
        &mut global_clock,
        &mut event_queue,
        &mut quest_board,
        &mut activity,
        &mut pawn_query,
        &capabilities_query,
        &mut known_recipes_query,
        report.as_deref_mut(),
        log_settings
            .as_deref()
            .map(|settings| settings.stdout_enabled)
            .unwrap_or(true),
    );
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

fn build_headless_report(app: &mut App, termination: &HeadlessTermination, elapsed_secs: f64) -> String {
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
        run_headless_with_target_ticks, HeadlessTermination, SimLifeState, SIMLIFE_GRASS_MAX,
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
    let sim_status = format!(
        "Sim tick: {}  Speed: {:.2}x{}\nKeys: R reset  [ ] speed  P pause  Arrows/WASD pan  Wheel zoom",
        seq,
        scale.0,
        pause
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
        FoodReservation { portions: 8 },
        Position { chunk: chunk_a },
        ItemIdentity { item_id: id_food_a, created_at_causal_seq: 0 },
        ItemHistory::default(),
        Sprite::from_color(Color::srgb(0.9, 0.5, 0.1), Vec2::splat(SPRITE_FOOD)),
        Transform::from_translation(chunk_to_translation(&chunk_a, 0.0)),
        Name::new("Food_A"),
    ));
    let id_water_a = allocator.alloc();
    commands.spawn((
        WaterSource { portions: 8 },
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
        FoodReservation { portions: 8 },
        Position { chunk: chunk_b },
        ItemIdentity { item_id: id_food_b, created_at_causal_seq: 0 },
        ItemHistory::default(),
        Sprite::from_color(Color::srgb(0.9, 0.5, 0.1), Vec2::splat(SPRITE_FOOD)),
        Transform::from_translation(chunk_to_translation(&chunk_b, 0.0)),
        Name::new("Food_B"),
    ));
    let id_water_b = allocator.alloc();
    commands.spawn((
        WaterSource { portions: 8 },
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
    let seq = global_clock.causal_seq();
    if seq <= *last_tick {
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
}

/// Chunk grid extent (0..=CHUNK_EXTENT). Used for respawn bounds.
const CHUNK_EXTENT: u32 = WORLD_CHUNK_EXTENT;

/// Target counts for respawn: maintain at least this many food and water entities in the world.
const TARGET_FOOD_COUNT: usize = 2;
const TARGET_WATER_COUNT: usize = 2;
const SIMLIFE_GRASS_MAX: u32 = 10;
const SIMLIFE_GRASS_GROWTH_PERIOD: u64 = 10;
const SIMLIFE_BASE_FOOD_PORTIONS: u32 = 4;
const SIMLIFE_GRASS_TO_FOOD_DIVISOR: u32 = 2;
const SIMLIFE_MAX_FOOD_PORTIONS: u32 = 12;

/// Tracks last sim tick we ran respawn. Ensures we run respawn once per tick.
#[derive(Resource, Default)]
struct RespawnState {
    last_tick: u64,
}

/// SimLife placeholder: per-chunk grass pressure read by surface resource systems.
#[derive(Resource, Debug, Clone)]
struct SimLifeState {
    last_tick: u64,
    grass_per_chunk: HashMap<ChunkId, u32>,
}

impl Default for SimLifeState {
    fn default() -> Self {
        Self {
            last_tick: 0,
            grass_per_chunk: HashMap::new(),
        }
    }
}

fn advance_simlife_grass(current_seq: u64, simlife: &mut SimLifeState) {
    if current_seq <= simlife.last_tick {
        return;
    }
    simlife.last_tick = current_seq;

    for x in 0..=CHUNK_EXTENT {
        for y in 0..=CHUNK_EXTENT {
            let chunk = ChunkId(x, y);
            let entry = simlife.grass_per_chunk.entry(chunk).or_insert(0);
            if (current_seq + x as u64 + y as u64) % SIMLIFE_GRASS_GROWTH_PERIOD == 0 {
                *entry = (*entry + 1).min(SIMLIFE_GRASS_MAX);
            }
        }
    }
}

fn food_portions_from_grass(grass: u32) -> u32 {
    (SIMLIFE_BASE_FOOD_PORTIONS + grass / SIMLIFE_GRASS_TO_FOOD_DIVISOR)
        .min(SIMLIFE_MAX_FOOD_PORTIONS)
}

fn simlife_tick_system(
    global_clock: Res<GlobalTickClock>,
    mut simlife: ResMut<SimLifeState>,
) {
    // Causal ordering: run after sim_tick_driver, then respawn reads same-tick SimLife state.
    advance_simlife_grass(global_clock.causal_seq(), &mut simlife);
}

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
    let current = global_clock.causal_seq();
    if current <= state.last_tick {
        return;
    }
    state.last_tick = current;

    let food_chunks: HashSet<_> = food_query.iter().map(|p| p.chunk).collect();
    let water_chunks: HashSet<_> = water_query.iter().map(|p| p.chunk).collect();
    let occupied: HashSet<_> = food_chunks.union(&water_chunks).copied().collect();

    let need_food = food_chunks.len() < TARGET_FOOD_COUNT;
    let need_water = water_chunks.len() < TARGET_WATER_COUNT;

    let food_chunk = if need_food {
        select_empty_chunk(current, &occupied)
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
        if let Some(chunk) = select_empty_chunk(current.wrapping_add(1), &occupied_after) {
            let id = allocator.alloc();
            commands.spawn((
                WaterSource { portions: 8 },
                Position { chunk },
                ItemIdentity { item_id: id, created_at_causal_seq: current },
                ItemHistory::default(),
                Sprite::from_color(Color::srgb(0.2, 0.85, 0.95), Vec2::splat(SPRITE_WATER)),
                Transform::from_translation(chunk_to_translation(&chunk, 0.0)),
                Name::new("Water_respawn"),
            ));
        }
    }
}

fn sim_tick_driver(
    time: Res<Time>,
    mut global_clock: ResMut<GlobalTickClock>,
    mut accumulator: ResMut<SimTickAccumulator>,
    scale: Res<SimTimeScale>,
    mut event_queue: ResMut<CausalEventQueue>,
    mut quest_board: ResMut<QuestBoard>,
    mut activity: ResMut<ActivityLog>,
    mut pawn_query: Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
    capabilities_query: Query<&Capabilities>,
    mut known_recipes_query: Query<&mut KnownRecipes>,
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
            &mut pawn_query,
            &capabilities_query,
            &mut known_recipes_query,
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
