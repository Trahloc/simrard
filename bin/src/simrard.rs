use bevy::prelude::*;
use bevy::ecs::message::{MessageReader, Messages};
use bevy::sprite::Sprite;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;
use simrard_lib_utility_ai::{BigBrainPlugin, BigBrainSet};
use simrard_lib_ai::{self as ai, build_pawn_brain, ActivityLog, PawnAIPlugin};
use simrard_lib_causal::{heartbeat, CausalEventQueue, CausalPlugin};
use simrard_lib_charter::{
    charter_watchguard_system, CharterFlashEvent, ChunkId, FrameWriteLog, SpatialCharter,
};
use simrard_lib_pawn::{
    Capabilities, FoodReservation, ItemHistory, ItemIdAllocator, ItemIdentity, NeuralNetworkComponent,
    Position, Quest, QuestBoard, QuestStatus, RestSpot, SimulationLogSettings,
    SimulationReport, WaterSource,
};
use simrard_lib_time::{
    CausalClock, GlobalTickClock, SimTickAccumulator, SimTimeScale, TimePlugin,
    SIM_TICKS_PER_SECOND_AT_1X,
};
use simrard_lib_transforms::TransformsPlugin;

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
        .init_resource::<RespawnState>();
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
                chunk_grid_gizmo_system,
                pawn_dominant_drive_color_system,
                charter_flash_spawn_system,
                charter_flash_tick_system,
                ui_panel_update_system,
            )
                .chain(),
        );

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
        .init_resource::<SimulationReport>()
        .insert_resource(SimulationLogSettings { stdout_enabled: false })
        .add_systems(Startup, (setup, initialize_report_baseline).chain())
        .add_systems(
            PreUpdate,
            (
                ai::pawn_death_system,
                ApplyDeferred,
                headless_tick_driver,
                resource_respawn_system,
            )
                .chain()
                .before(BigBrainSet::Scorers),
        );
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
        Some(activity),
        report.as_deref_mut(),
        stdout_enabled,
    );
    if let Some(report) = report.as_deref_mut() {
        report.bump("sim_ticks_advanced");
    }
    run_curiosity_step(quest_board, pawn_query);
}

fn headless_tick_driver(
    mut global_clock: ResMut<GlobalTickClock>,
    mut event_queue: ResMut<CausalEventQueue>,
    mut quest_board: ResMut<QuestBoard>,
    mut activity: ResMut<ActivityLog>,
    mut pawn_query: Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
    mut report: Option<ResMut<SimulationReport>>,
    log_settings: Option<Res<SimulationLogSettings>>,
) {
    advance_simulation_one_tick(
        &mut global_clock,
        &mut event_queue,
        &mut quest_board,
        &mut activity,
        &mut pawn_query,
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
    let extent = 12; // 0..=11 chunks so (0,0) and (10,10) are inside
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
        force_panic_error_handlers, run_headless_with_target_ticks, HeadlessTermination,
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
}

fn ui_panel_update_system(
    global_clock: Res<GlobalTickClock>,
    scale: Res<SimTimeScale>,
    quest_board: Res<QuestBoard>,
    activity: Res<ActivityLog>,
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
        "Sim tick: {}  Speed: {:.2}x{}\nKeys: R reset  [ ] speed  P pause",
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
                    .map(|q| format!("  {} @ {:?} – {:?}", q.need, q.chunk, q.status)),
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

fn chunk_to_translation(chunk: &ChunkId, z: f32) -> Vec3 {
    Vec3::new(
        chunk.0 as f32 * CHUNK_PIXEL,
        chunk.1 as f32 * CHUNK_PIXEL,
        z,
    )
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
            Sprite::from_color(Color::srgb(0.2, 0.75, 0.3), Vec2::splat(SPRITE_PAWN)),
            Transform::from_translation(chunk_to_translation(&chunk_a, 0.0) + offset),
            Name::new(format!("Pawn_A_{}", i)),
            PawnVisual,
        ));
    }

    // Cluster B: food at (10,10), water at (9,10) — never same chunk.
    let chunk_b = ChunkId(10, 10);
    let water_b_chunk = ChunkId(9, 10);
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
            Sprite::from_color(Color::srgb(0.2, 0.75, 0.3), Vec2::splat(SPRITE_PAWN)),
            Transform::from_translation(chunk_to_translation(&chunk_b, 0.0) + offset),
            Name::new(format!("Pawn_B_{}", i)),
            PawnVisual,
        ));
    }
}

fn run_curiosity_step(
    quest_board: &mut QuestBoard,
    pawn_query: &mut Query<(Entity, &Name, &Position, &mut NeuralNetworkComponent)>,
) {
    for (entity, _name, position, mut nn) in pawn_query.iter_mut() {
        nn.curiosity += 0.001;
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
    }
}

/// Chunk grid extent (0..=CHUNK_EXTENT). Used for respawn bounds.
const CHUNK_EXTENT: u32 = 11;

/// Target counts for respawn: maintain at least this many food and water entities in the world.
const TARGET_FOOD_COUNT: usize = 2;
const TARGET_WATER_COUNT: usize = 2;

/// Tracks last sim tick we ran respawn. Ensures we run respawn once per tick.
#[derive(Resource, Default)]
struct RespawnState {
    last_tick: u64,
}

/// Spawns food and water at random empty chunks when below target. Food and water never share a chunk.
/// Deterministic from causal_seq (no rand crate). Run after sim_tick_driver.
fn resource_respawn_system(
    global_clock: Res<GlobalTickClock>,
    mut state: ResMut<RespawnState>,
    mut commands: Commands,
    mut allocator: ResMut<ItemIdAllocator>,
    food_query: Query<&Position, With<FoodReservation>>,
    water_query: Query<&Position, With<WaterSource>>,
) {
    let current = global_clock.causal_seq();
    if current <= state.last_tick {
        return;
    }
    state.last_tick = current;

    let food_chunks: std::collections::HashSet<_> =
        food_query.iter().map(|p| p.chunk).collect();
    let water_chunks: std::collections::HashSet<_> =
        water_query.iter().map(|p| p.chunk).collect();
    let occupied: std::collections::HashSet<_> =
        food_chunks.union(&water_chunks).copied().collect();
    let empty: Vec<ChunkId> = (0..=CHUNK_EXTENT)
        .flat_map(|x| (0..=CHUNK_EXTENT).map(move |y| ChunkId(x, y)))
        .filter(|c| !occupied.contains(c))
        .collect();

    if empty.is_empty() {
        return;
    }

    let need_food = food_chunks.len() < TARGET_FOOD_COUNT;
    let need_water = water_chunks.len() < TARGET_WATER_COUNT;

    let food_chunk = if need_food {
        let idx = (current as usize) % empty.len();
        Some(empty[idx])
    } else {
        None
    };

    if let Some(chunk) = food_chunk {
        let id = allocator.alloc();
        commands.spawn((
            FoodReservation { portions: 8 },
            Position { chunk },
            ItemIdentity { item_id: id, created_at_causal_seq: current },
            ItemHistory::default(),
            Sprite::from_color(Color::srgb(0.9, 0.5, 0.1), Vec2::splat(SPRITE_FOOD)),
            Transform::from_translation(chunk_to_translation(&chunk, 0.0)),
            Name::new("Food_respawn"),
        ));
    }

    if need_water {
        let occupied_after: std::collections::HashSet<_> = food_chunks
            .iter()
            .copied()
            .chain(water_chunks.iter().copied())
            .chain(food_chunk)
            .collect();
        let empty_after: Vec<ChunkId> = (0..=CHUNK_EXTENT)
            .flat_map(|x| (0..=CHUNK_EXTENT).map(move |y| ChunkId(x, y)))
            .filter(|c| !occupied_after.contains(c))
            .collect();
        if let Some(chunk) = empty_after.get((current.wrapping_add(1) as usize) % empty_after.len().max(1)) {
            let id = allocator.alloc();
            commands.spawn((
                WaterSource { portions: 8 },
                Position { chunk: *chunk },
                ItemIdentity { item_id: id, created_at_causal_seq: current },
                ItemHistory::default(),
                Sprite::from_color(Color::srgb(0.2, 0.85, 0.95), Vec2::splat(SPRITE_WATER)),
                Transform::from_translation(chunk_to_translation(chunk, 0.0)),
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
