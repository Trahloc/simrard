//! # ECS Mirror — Phase 4.D1 Foundation
//!
//! **Strategy**: Push sync. After each sim tick, a Bevy system (`push_ecs_snapshot_system`)
//! reads relevant ECS components and writes a point-in-time snapshot into an in-memory
//! DuckDB database. The snapshot is complete (all pawns, resources, quests) and immutable
//! once written — it is never updated in place; each tick produces new rows.
//!
//! **Policy**: This crate requires a system DuckDB installation. We explicitly do
//! not support `features = ["bundled"]` because it compiles DuckDB C++ sources and
//! can saturate developer machines on clean builds.
//!
//! **Causal ordering guarantee**: The push system runs after sim tick systems.
//! Snapshots reflect post-tick state at the sampled `causal_seq`.

use bevy::prelude::*;
use duckdb::{Connection, Result as DuckResult, ToSql};
use simrard_lib_charter::ChunkId;
use simrard_lib_pawn::{
    Capabilities, FoodReservation, KnownRecipes, NeuralNetworkComponent, Position, QuestBoard,
    QuestStatus, RestSpot, WaterSource,
};
use simrard_lib_time::{CausalClock, GlobalTickClock};
use std::sync::Mutex;

const PROVIDER_RANK_DRIVE_WEIGHT: f32 = 0.75;
const PROVIDER_RANK_PROXIMITY_WEIGHT: f32 = 0.25;

/// One row in `pawn_snapshot`.
#[derive(Debug, Clone)]
pub struct PawnSnapshotRow {
    pub causal_seq: u64,
    pub entity_index: u32,
    pub entity_generation: u32,
    pub chunk_x: i32,
    pub chunk_y: i32,
    pub hunger: f32,
    pub thirst: f32,
    pub fatigue: f32,
    pub curiosity: f32,
    pub social: f32,
    pub fear: f32,
    pub industriousness: f32,
    pub comfort: f32,
    pub known_recipes: String,
    pub capabilities: String,
}

/// One row in `resource_snapshot`.
#[derive(Debug, Clone)]
pub struct ResourceSnapshotRow {
    pub causal_seq: u64,
    pub entity_index: u32,
    pub entity_generation: u32,
    pub resource_type: String,
    pub chunk_x: i32,
    pub chunk_y: i32,
    pub portions: u32,
}

/// One row in `quest_snapshot`.
#[derive(Debug, Clone)]
pub struct QuestSnapshotRow {
    pub causal_seq: u64,
    pub quest_index: usize,
    pub need: String,
    pub requester_index: u32,
    pub requester_generation: u32,
    pub chunk_x: i32,
    pub chunk_y: i32,
    pub status: String,
    pub provider_index: Option<u32>,
    pub provider_generation: Option<u32>,
}

/// Candidate input used for DuckDB-backed provider ranking in Phase 4.D2.
#[derive(Debug, Clone)]
pub struct ProviderCandidateInput {
    pub candidate_id: u32,
    pub drive: f32,
    pub proximity: f32,
    pub distance: u32,
    pub can_eat: bool,
    pub can_drink: bool,
    pub can_rest: bool,
}

/// Bevy resource holding an in-memory DuckDB connection for ECS mirroring.
#[derive(Resource)]
pub struct EcsMirror {
    conn: Mutex<Connection>,
    pub snapshot_count: u64,
}

impl EcsMirror {
    pub fn new() -> DuckResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::create_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            snapshot_count: 0,
        })
    }

    fn create_schema(conn: &Connection) -> DuckResult<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS pawn_snapshot (
                causal_seq          UBIGINT NOT NULL,
                entity_index        UINTEGER NOT NULL,
                entity_generation   UINTEGER NOT NULL,
                chunk_x             INTEGER NOT NULL,
                chunk_y             INTEGER NOT NULL,
                hunger              FLOAT NOT NULL,
                thirst              FLOAT NOT NULL,
                fatigue             FLOAT NOT NULL,
                curiosity           FLOAT NOT NULL,
                social              FLOAT NOT NULL,
                fear                FLOAT NOT NULL,
                industriousness     FLOAT NOT NULL,
                comfort             FLOAT NOT NULL,
                known_recipes       VARCHAR NOT NULL,
                capabilities        VARCHAR NOT NULL
            );

            CREATE TABLE IF NOT EXISTS resource_snapshot (
                causal_seq          UBIGINT NOT NULL,
                entity_index        UINTEGER NOT NULL,
                entity_generation   UINTEGER NOT NULL,
                resource_type       VARCHAR NOT NULL,
                chunk_x             INTEGER NOT NULL,
                chunk_y             INTEGER NOT NULL,
                portions            UINTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS quest_snapshot (
                causal_seq              UBIGINT NOT NULL,
                quest_index             UINTEGER NOT NULL,
                need                    VARCHAR NOT NULL,
                requester_index         UINTEGER NOT NULL,
                requester_generation    UINTEGER NOT NULL,
                chunk_x                 INTEGER NOT NULL,
                chunk_y                 INTEGER NOT NULL,
                status                  VARCHAR NOT NULL,
                provider_index          UINTEGER,
                provider_generation     UINTEGER
            );
            ",
        )
    }

    pub fn push_pawn_rows(&mut self, rows: &[PawnSnapshotRow]) -> DuckResult<()> {
        let conn = self.conn.lock().expect("EcsMirror: conn lock poisoned");
        let mut stmt = conn.prepare(
            "INSERT INTO pawn_snapshot VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )?;
        for r in rows {
            let param_list: [&dyn ToSql; 15] = [
                &r.causal_seq,
                &r.entity_index,
                &r.entity_generation,
                &r.chunk_x,
                &r.chunk_y,
                &r.hunger,
                &r.thirst,
                &r.fatigue,
                &r.curiosity,
                &r.social,
                &r.fear,
                &r.industriousness,
                &r.comfort,
                &r.known_recipes,
                &r.capabilities,
            ];
            stmt.execute(param_list)?;
        }
        Ok(())
    }

    pub fn push_resource_rows(&mut self, rows: &[ResourceSnapshotRow]) -> DuckResult<()> {
        let conn = self.conn.lock().expect("EcsMirror: conn lock poisoned");
        let mut stmt = conn.prepare("INSERT INTO resource_snapshot VALUES (?,?,?,?,?,?,?)")?;
        for r in rows {
            let param_list: [&dyn ToSql; 7] = [
                &r.causal_seq,
                &r.entity_index,
                &r.entity_generation,
                &r.resource_type,
                &r.chunk_x,
                &r.chunk_y,
                &r.portions,
            ];
            stmt.execute(param_list)?;
        }
        Ok(())
    }

    pub fn push_quest_rows(&mut self, rows: &[QuestSnapshotRow]) -> DuckResult<()> {
        let conn = self.conn.lock().expect("EcsMirror: conn lock poisoned");
        let mut stmt = conn.prepare("INSERT INTO quest_snapshot VALUES (?,?,?,?,?,?,?,?,?,?)")?;
        for r in rows {
            let quest_index_u32 = r.quest_index as u32;
            let quest_index_param: &dyn ToSql = &quest_index_u32;
            let param_list: [&dyn ToSql; 10] = [
                &r.causal_seq,
                quest_index_param,
                &r.need,
                &r.requester_index,
                &r.requester_generation,
                &r.chunk_x,
                &r.chunk_y,
                &r.status,
                &r.provider_index,
                &r.provider_generation,
            ];
            stmt.execute(param_list)?;
        }
        Ok(())
    }

    pub fn pawn_row_count_at(&self, causal_seq: u64) -> DuckResult<u64> {
        let conn = self.conn.lock().expect("EcsMirror: conn lock poisoned");
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM pawn_snapshot WHERE causal_seq = ?")?;
        let count: u64 = stmt.query_row([causal_seq], |row| row.get(0))?;
        Ok(count)
    }

    pub fn resource_row_count_at(&self, causal_seq: u64) -> DuckResult<u64> {
        let conn = self.conn.lock().expect("EcsMirror: conn lock poisoned");
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM resource_snapshot WHERE causal_seq = ?")?;
        let count: u64 = stmt.query_row([causal_seq], |row| row.get(0))?;
        Ok(count)
    }

    pub fn quest_row_count_at(&self, causal_seq: u64) -> DuckResult<u64> {
        let conn = self.conn.lock().expect("EcsMirror: conn lock poisoned");
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM quest_snapshot WHERE causal_seq = ?")?;
        let count: u64 = stmt.query_row([causal_seq], |row| row.get(0))?;
        Ok(count)
    }
}

/// Rank provider candidates for a quest need using a small DuckDB-scored intent vector.
///
/// The intent vector is `[drive, proximity]`, where `drive` is the normalized
/// need-relevant readiness and `proximity` is typically `1 / (distance + 1)`.
/// DuckDB computes a weighted euclidean distance to the ideal vector `[1, 1]`
/// and returns the best candidate id.
pub fn rank_provider_candidates_for_need(
    need: &str,
    min_drive: f32,
    candidates: &[ProviderCandidateInput],
) -> DuckResult<Option<u32>> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(
        "
        CREATE TEMP TABLE provider_candidates (
            candidate_id    UINTEGER NOT NULL,
            drive           FLOAT NOT NULL,
            proximity       FLOAT NOT NULL,
            distance        UINTEGER NOT NULL,
            can_eat         BOOLEAN NOT NULL,
            can_drink       BOOLEAN NOT NULL,
            can_rest        BOOLEAN NOT NULL
        );
        ",
    )?;

    let mut stmt = conn.prepare(
        "INSERT INTO provider_candidates VALUES (?,?,?,?,?,?,?)",
    )?;
    for candidate in candidates {
        let param_list: [&dyn ToSql; 7] = [
            &candidate.candidate_id,
            &candidate.drive,
            &candidate.proximity,
            &candidate.distance,
            &candidate.can_eat,
            &candidate.can_drink,
            &candidate.can_rest,
        ];
        stmt.execute(param_list)?;
    }

    let mut rank_stmt = conn.prepare(
        "
        SELECT candidate_id
        FROM provider_candidates
        WHERE drive >= ?
          AND ((? = 'food' AND can_eat)
            OR (? = 'water' AND can_drink)
            OR (? = 'rest' AND can_rest))
        ORDER BY
            1.0 - sqrt(
                pow((1.0 - drive) * ?, 2) +
                pow((1.0 - proximity) * ?, 2)
            ) DESC,
            distance ASC,
            candidate_id ASC
        LIMIT 1
        ",
    )?;

    let min_drive_param: &dyn ToSql = &min_drive;
    let need_param: &dyn ToSql = &need;
    let params: [&dyn ToSql; 6] = [
        min_drive_param,
        need_param,
        need_param,
        need_param,
        &PROVIDER_RANK_DRIVE_WEIGHT,
        &PROVIDER_RANK_PROXIMITY_WEIGHT,
    ];

    match rank_stmt.query_row(params, |row| row.get(0)) {
        Ok(candidate_id) => Ok(Some(candidate_id)),
        Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error),
    }
}

fn chunk_xy(chunk: &ChunkId) -> (i32, i32) {
    (chunk.0 as i32, chunk.1 as i32)
}

fn sorted_join(items: impl Iterator<Item = impl AsRef<str>>) -> String {
    let mut v: Vec<String> = items.map(|s| s.as_ref().to_owned()).collect();
    v.sort_unstable();
    v.join(",")
}

pub fn push_ecs_snapshot_system(
    global_clock: Res<GlobalTickClock>,
    mut mirror: ResMut<EcsMirror>,
    pawn_query: Query<(
        Entity,
        &Position,
        &NeuralNetworkComponent,
        Option<&KnownRecipes>,
        Option<&Capabilities>,
    )>,
    food_query: Query<(Entity, &Position, &FoodReservation)>,
    water_query: Query<(Entity, &Position, &WaterSource)>,
    rest_query: Query<(Entity, &Position), With<RestSpot>>,
    quest_board: Res<QuestBoard>,
) {
    let seq = global_clock.causal_seq();

    let pawn_rows: Vec<PawnSnapshotRow> = pawn_query
        .iter()
        .map(|(entity, pos, nn, recipes, caps)| {
            let (cx, cy) = chunk_xy(&pos.chunk);
            PawnSnapshotRow {
                causal_seq: seq,
                entity_index: entity.index_u32(),
                entity_generation: entity.generation().to_bits(),
                chunk_x: cx,
                chunk_y: cy,
                hunger: nn.hunger,
                thirst: nn.thirst,
                fatigue: nn.fatigue,
                curiosity: nn.curiosity,
                social: nn.social,
                fear: nn.fear,
                industriousness: nn.industriousness,
                comfort: nn.comfort,
                known_recipes: recipes
                    .map(|r| sorted_join(r.recipes.iter()))
                    .unwrap_or_default(),
                capabilities: caps
                    .map(|c| sorted_join(c.can_do.iter()))
                    .unwrap_or_default(),
            }
        })
        .collect();

    let mut resource_rows: Vec<ResourceSnapshotRow> = Vec::new();
    for (entity, pos, food) in &food_query {
        let (cx, cy) = chunk_xy(&pos.chunk);
        resource_rows.push(ResourceSnapshotRow {
            causal_seq: seq,
            entity_index: entity.index_u32(),
            entity_generation: entity.generation().to_bits(),
            resource_type: "food".to_owned(),
            chunk_x: cx,
            chunk_y: cy,
            portions: food.portions,
        });
    }
    for (entity, pos, water) in &water_query {
        let (cx, cy) = chunk_xy(&pos.chunk);
        resource_rows.push(ResourceSnapshotRow {
            causal_seq: seq,
            entity_index: entity.index_u32(),
            entity_generation: entity.generation().to_bits(),
            resource_type: "water".to_owned(),
            chunk_x: cx,
            chunk_y: cy,
            portions: water.portions,
        });
    }
    for (entity, pos) in &rest_query {
        let (cx, cy) = chunk_xy(&pos.chunk);
        resource_rows.push(ResourceSnapshotRow {
            causal_seq: seq,
            entity_index: entity.index_u32(),
            entity_generation: entity.generation().to_bits(),
            resource_type: "rest".to_owned(),
            chunk_x: cx,
            chunk_y: cy,
            portions: 0,
        });
    }

    let quest_rows: Vec<QuestSnapshotRow> = quest_board
        .active_quests
        .iter()
        .enumerate()
        .map(|(idx, quest)| {
            let (cx, cy) = chunk_xy(&quest.chunk);
            let (status, provider_index, provider_generation) = match &quest.status {
                QuestStatus::Open => ("open".to_owned(), None, None),
                QuestStatus::InProgress { provider } => (
                    "in_progress".to_owned(),
                    Some(provider.index_u32()),
                    Some(provider.generation().to_bits()),
                ),
                QuestStatus::Completed => ("completed".to_owned(), None, None),
            };
            QuestSnapshotRow {
                causal_seq: seq,
                quest_index: idx,
                need: quest.need.clone(),
                requester_index: quest.requester.index_u32(),
                requester_generation: quest.requester.generation().to_bits(),
                chunk_x: cx,
                chunk_y: cy,
                status,
                provider_index,
                provider_generation,
            }
        })
        .collect();

    mirror
        .push_pawn_rows(&pawn_rows)
        .expect("ECS mirror: pawn snapshot insert failed");
    mirror
        .push_resource_rows(&resource_rows)
        .expect("ECS mirror: resource snapshot insert failed");
    mirror
        .push_quest_rows(&quest_rows)
        .expect("ECS mirror: quest snapshot insert failed");
    mirror.snapshot_count += 1;
}

pub struct MirrorPlugin;

impl Plugin for MirrorPlugin {
    fn build(&self, app: &mut App) {
        let mirror = EcsMirror::new().expect("ECS mirror: failed to open DuckDB connection");
        app.insert_resource(mirror);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_tables_created() {
        let mirror = EcsMirror::new().expect("EcsMirror::new failed");
        assert_eq!(mirror.pawn_row_count_at(0).unwrap(), 0);
        assert_eq!(mirror.resource_row_count_at(0).unwrap(), 0);
        assert_eq!(mirror.quest_row_count_at(0).unwrap(), 0);
    }

    #[test]
    fn pawn_snapshot_round_trip() {
        let mut mirror = EcsMirror::new().expect("EcsMirror::new failed");
        let row = PawnSnapshotRow {
            causal_seq: 42,
            entity_index: 1,
            entity_generation: 0,
            chunk_x: 3,
            chunk_y: 7,
            hunger: 0.8,
            thirst: 0.5,
            fatigue: 0.3,
            curiosity: 1.0,
            social: 0.9,
            fear: 0.1,
            industriousness: 0.6,
            comfort: 0.7,
            known_recipes: "Fire".to_owned(),
            capabilities: "Eat,Rest".to_owned(),
        };
        mirror.push_pawn_rows(&[row]).expect("push_pawn_rows failed");
        assert_eq!(mirror.pawn_row_count_at(42).unwrap(), 1);
    }

    #[test]
    fn provider_ranking_prefers_best_vector_match() {
        let candidates = vec![
            ProviderCandidateInput {
                candidate_id: 0,
                drive: 0.95,
                proximity: 1.0 / 5.0,
                distance: 4,
                can_eat: true,
                can_drink: false,
                can_rest: false,
            },
            ProviderCandidateInput {
                candidate_id: 1,
                drive: 0.60,
                proximity: 1.0,
                distance: 0,
                can_eat: true,
                can_drink: false,
                can_rest: false,
            },
        ];

        let selected = rank_provider_candidates_for_need("food", 0.3, &candidates)
            .expect("DuckDB provider ranking failed");
        assert_eq!(selected, Some(0));
    }

    #[test]
    fn provider_ranking_returns_none_when_no_capability_match() {
        let candidates = vec![ProviderCandidateInput {
            candidate_id: 0,
            drive: 0.9,
            proximity: 1.0,
            distance: 0,
            can_eat: false,
            can_drink: true,
            can_rest: false,
        }];

        let selected = rank_provider_candidates_for_need("food", 0.3, &candidates)
            .expect("DuckDB provider ranking failed");
        assert_eq!(selected, None);
    }
}
