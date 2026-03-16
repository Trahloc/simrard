//! Tier 4 Reflex — Insect layer.
//!
//! Minimal viable reflex agents. No LTW, no Concept Space.
//! STW only: a small fixed-size array of drive values that decay each update (habituation).
//!
//! Drives: hunger, fear, reproduction-urgency.
//!
//! Behaviour:
//!   - Eat: consume local GS biomass (grass) → gain energy.
//!   - Decompose: on death leave nutrient trace (feeds Tier 6 fungal field via chemistry).
//!   - Move: random walk biased away from fear stimulus.
//!   - Reproduce: when energy surplus, spawn one offspring nearby (capped at T4_MAX_POPULATION).
//!
//! Charter integration: every write (eat/decompose/reproduce) obtains a charter lease or is
//! denied and skipped — no silent fallback.
//!
//! Hypergraph hook: clustering output at the insect's chunk biases reproduction probability.
//! High causal_volume raises fear drive.

use bevy::prelude::*;
use simrard_lib_charter::{ChunkId, LeaseIntent, SpatialCharter, SpatialLease};
use simrard_lib_hypergraph::HypergraphSubstrate;
use std::any::TypeId;
use std::collections::HashMap;

// ──────────────────────────────────────────────────────────────────────────
// Public configuration constants (tunable by integration layer)
// ──────────────────────────────────────────────────────────────────────────

/// Maximum live insect population.
pub const T4_MAX_POPULATION: usize = 10_000;
/// Initial sparse seed count when the layer is first activated.
pub const T4_INITIAL_SEEDS: usize = 500;
/// Number of ticks between Tier-4 updates (10 Hz @ 1000 tps).
pub const T4_UPDATE_INTERVAL_TICKS: u64 = 100;
/// Energy gained per unit of biomass consumed.
pub const T4_ENERGY_PER_GRASS: f32 = 2.0;
/// Energy consumed per tick by metabolic cost.
pub const T4_METABOLIC_COST: f32 = 0.05;
/// Energy threshold to attempt reproduction.
pub const T4_REPRODUCTION_THRESHOLD: f32 = 4.0;
/// Energy spent per reproduction event.
pub const T4_REPRODUCTION_COST: f32 = 2.5;
/// Fraction of local biomass consumed per eat event.
pub const T4_GRASS_CONSUMPTION_FRACTION: f32 = 0.25;
/// Decomposition chemistry deposit per dead insect (normalized 0..1).
pub const T4_DECOMP_DEPOSIT: f32 = 0.04;
/// STW decay rate per update tick (habituation).
pub const T4_STW_DECAY: f32 = 0.15;
/// Chunk extent mirror from world constants — used for bounds checking.
/// Must match WORLD_CHUNK_EXTENT in `simrard_lib_pawn`.
pub const T4_CHUNK_EXTENT: u32 = 255;
/// Fear threshold above which fear drives movement.
pub const T4_FEAR_FLEE_THRESHOLD: f32 = 0.6;
/// Hypergraph causal volume scale factor for fear drive.
pub const T4_FEAR_CAUSAL_SCALE: f32 = 0.8;
/// Hypergraph clustering scale factor for reproduction bias.
pub const T4_REPRO_CLUSTER_SCALE: f32 = 0.4;
/// Maximum nutrient noise-floor cap deposited during decomposition.
pub const T4_DECOMP_MAX: f32 = 0.4;

// ──────────────────────────────────────────────────────────────────────────
// Zero-sized write markers for charter intent declarations
// ──────────────────────────────────────────────────────────────────────────

/// Marker: this chunk's grass biomass field is being written.
pub struct Tier4GrassConsumeWrite;
/// Marker: this chunk's chemistry noise floor is being written (decomposition).
pub struct Tier4DecompWrite;
/// Marker: a new insect is being spawned onto this chunk.
pub struct Tier4ReproduceWrite;

// ──────────────────────────────────────────────────────────────────────────
// Insect agent data
// ──────────────────────────────────────────────────────────────────────────

/// One reflex insect. Stored inline in `Tier4State::insects`.
#[derive(Debug, Clone)]
pub struct Insect {
    pub chunk: ChunkId,
    /// Current energy store. Agent dies when energy drops to 0.
    pub energy: f32,
    /// Fear drive: 0=calm, 1=maximal fear.
    pub fear: f32,
    /// Reproduction urgency: increases with surplus energy.
    pub repro_urgency: f32,
    /// Short-term working memory (STW): 8-cell fixed array, habituates by decay.
    pub stw: [f32; 8],
    /// Age in update ticks.
    pub age: u64,
}

impl Insect {
    fn new(chunk: ChunkId, energy: f32) -> Self {
        Self {
            chunk,
            energy,
            fear: 0.0,
            repro_urgency: 0.0,
            stw: [0.0; 8],
            age: 0,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Population metrics (exported for reporting)
// ──────────────────────────────────────────────────────────────────────────

/// Snapshot metrics captured after each Tier-4 update tick.
#[derive(Debug, Clone)]
pub struct Tier4TickMetrics {
    /// Simulated tick index at the time of capture.
    pub tick: u64,
    /// Live insect count.
    pub population: usize,
    /// Eat events that succeeded (lease granted).
    pub eat_grants: u64,
    /// Eat events that were denied (charter conflict).
    pub eat_denials: u64,
    /// Reproduce events that succeeded.
    pub repro_grants: u64,
    /// Reproduce events that failed (cap or charter).
    pub repro_denials: u64,
    /// Deaths this tick.
    pub deaths: u64,
    /// Average energy across live insects (0 if empty).
    pub avg_energy: f32,
}

// ──────────────────────────────────────────────────────────────────────────
// Tier4State — the main Bevy Resource
// ──────────────────────────────────────────────────────────────────────────

/// Bevy resource holding all living reflex insects and rolling metrics.
#[derive(Resource)]
pub struct Tier4State {
    pub insects: Vec<Insect>,
    /// Last causal_seq at which we ran an update.
    pub last_tick: u64,
    /// Whether the initial seed has been placed.
    pub seeded: bool,
    /// Rolling metrics: one entry per update tick, capped at 600 entries.
    pub metrics: Vec<Tier4TickMetrics>,
    /// Cumulative decomposition writes (for external reporting).
    pub decomp_total: u64,
}

impl Default for Tier4State {
    fn default() -> Self {
        Self {
            insects: Vec::new(),
            last_tick: 0,
            seeded: false,
            metrics: Vec::new(),
            decomp_total: 0,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Seeding
// ──────────────────────────────────────────────────────────────────────────

/// Seed the initial insect population at sparse positions across the 256×256 chunk grid.
/// Called once when `Tier4State::seeded` is false.
pub fn seed_tier4_population(state: &mut Tier4State, current_seq: u64) {
    let step = T4_CHUNK_EXTENT as usize / (T4_INITIAL_SEEDS as f64).sqrt() as usize;
    let step = step.max(4) as u32;
    let mut count = 0_usize;
    let mut x = step / 2;
    'outer: while x < T4_CHUNK_EXTENT {
        let mut y = step / 2;
        while y < T4_CHUNK_EXTENT {
            // Deterministic pseudo-random energy jitter from position hash.
            let hash = ((x as u64) << 16) ^ (y as u64) ^ (current_seq ^ 0xCAFE_BABE);
            let energy_jitter = (hash & 0xFF) as f32 / 255.0; // 0..1
            let energy = 1.5 + energy_jitter * 1.5; // 1.5..3.0
            state.insects.push(Insect::new(ChunkId(x, y), energy));
            count += 1;
            if count >= T4_INITIAL_SEEDS {
                break 'outer;
            }
            y += step;
        }
        x += step;
    }
    state.seeded = true;
}

// ──────────────────────────────────────────────────────────────────────────
// Main advance function
// ──────────────────────────────────────────────────────────────────────────

/// Advance the Tier-4 reflex layer by one logical tick.
///
/// Parameters:
/// - `current_seq`: current causal clock sequence.
/// - `grass_per_chunk`: mutable reference to GS-derived biomass map (Tier 5 link).
/// - `chemistry_noise_floor`: mutable reference to fungal chemistry map (Tier 6 link).
/// - `hypergraph`: read-only Tier 10 output (clustering → reproduction bias, causal_volume → fear).
/// - `charter`: mutable SpatialCharter for lease mediation.
///
/// Returns `Tier4TickMetrics` for the tick (empty if throttled).
pub fn advance_tier4(
    current_seq: u64,
    grass_per_chunk: &mut HashMap<ChunkId, u32>,
    chemistry_noise_floor: &mut HashMap<ChunkId, f32>,
    hypergraph: Option<&HypergraphSubstrate>,
    charter: &mut SpatialCharter,
    state: &mut Tier4State,
) -> Option<Tier4TickMetrics> {
    if current_seq <= state.last_tick {
        return None;
    }

    // Seed on first call.
    if !state.seeded {
        seed_tier4_population(state, current_seq);
    }

    // Throttle to T4_UPDATE_INTERVAL_TICKS.
    if current_seq.saturating_sub(state.last_tick) < T4_UPDATE_INTERVAL_TICKS {
        return None;
    }
    state.last_tick = current_seq;

    let mut eat_grants: u64 = 0;
    let mut eat_denials: u64 = 0;
    let mut repro_grants: u64 = 0;
    let mut repro_denials: u64 = 0;
    let mut deaths: u64 = 0;
    let mut new_insects: Vec<Insect> = Vec::new();
    let mut dead_chunks: Vec<ChunkId> = Vec::new();

    // Compute how many offspring can be born this tick without exceeding the cap.
    // Conservative: based on initial population; deaths during the tick are a bonus.
    let repro_budget: usize = T4_MAX_POPULATION.saturating_sub(state.insects.len());
    // Process every live insect; build survivor list.
    let mut survivors: Vec<Insect> = Vec::with_capacity(state.insects.len());

    for mut insect in state.insects.drain(..) {
        insect.age += 1;

        // ── STW habituation decay ───────────────────────────────────────
        for cell in insect.stw.iter_mut() {
            *cell = (*cell * (1.0 - T4_STW_DECAY)).max(0.0);
        }

        // ── Metabolic cost ──────────────────────────────────────────────
        insect.energy -= T4_METABOLIC_COST;
        if insect.energy <= 0.0 {
            // Death: deposit decomp trace.
            dead_chunks.push(insect.chunk);
            deaths += 1;
            continue;
        }

        // ── Fear drive update (Tier 10 hook) ────────────────────────────
        let (clustering, causal_vol) = match hypergraph {
            Some(hg) => match hg.output_for_chunk(insect.chunk.0, insect.chunk.1) {
                Some(out) => (out.clustering.clamp(0.0, 1.0), out.causal_volume.clamp(0.0, 1.0)),
                None => (0.0, 0.0),
            },
            None => (0.0, 0.0),
        };
        insect.fear = (insect.fear * 0.85 + causal_vol * T4_FEAR_CAUSAL_SCALE).clamp(0.0, 1.0);
        insect.stw[0] = (insect.stw[0] + insect.fear).clamp(0.0, 1.0); // fear channel

        // ── Eat action (Tier 5 GS link) ─────────────────────────────────
        let grass_available = *grass_per_chunk.get(&insect.chunk).unwrap_or(&0); // GS_SPARSE_FIELD_DEFAULT
        if grass_available > 0 {
            let lease_req = SpatialLease {
                primary: insect.chunk,
                fringe: vec![],
                intent: LeaseIntent {
                    reads: vec![],
                    writes: vec![TypeId::of::<Tier4GrassConsumeWrite>()],
                },
                granted_at_causal_seq: current_seq,
            };
            match charter.request_lease(lease_req, current_seq) {
                Ok(handle) => {
                    let consume = ((grass_available as f32 * T4_GRASS_CONSUMPTION_FRACTION) as u32)
                        .max(1)
                        .min(grass_available);
                    let new_grass = grass_available - consume;
                    if new_grass == 0 {
                        grass_per_chunk.remove(&insect.chunk);
                    } else {
                        grass_per_chunk.insert(insect.chunk, new_grass);
                    }
                    insect.energy = (insect.energy + T4_ENERGY_PER_GRASS * consume as f32).min(8.0);
                    insect.stw[1] = (insect.stw[1] + 0.5).clamp(0.0, 1.0); // satiation channel
                    charter.release_lease(handle);
                    eat_grants += 1;
                }
                Err(_) => {
                    eat_denials += 1;
                }
            }
        }

        // ── Movement (fear-biased random walk) ──────────────────────────
        if insect.fear > T4_FEAR_FLEE_THRESHOLD || insect.age % 5 == 0 {
            // Simple deterministic walk: hash position + age to get direction.
            let hash = ((insect.chunk.0 as u64) ^ ((insect.chunk.1 as u64) << 8) ^ insect.age)
                .wrapping_mul(0x9e37_79b9_7f4a_7c15);
            let dir = (hash >> 60) & 0x3; // 0..3
            let (dx, dy): (i32, i32) = match dir {
                0 => (1, 0),
                1 => (-1, 0),
                2 => (0, 1),
                _ => (0, -1),
            };
            let new_x = (insect.chunk.0 as i32 + dx).clamp(0, T4_CHUNK_EXTENT as i32) as u32;
            let new_y = (insect.chunk.1 as i32 + dy).clamp(0, T4_CHUNK_EXTENT as i32) as u32;
            insect.chunk = ChunkId(new_x, new_y);
        }

        // ── Reproduction (Tier 10 clustering hook) ──────────────────────
        if insect.energy > T4_REPRODUCTION_THRESHOLD {
            // Clustering increases willingness to reproduce.
            let repro_bias = 1.0 + clustering * T4_REPRO_CLUSTER_SCALE;
            let adjusted_threshold = T4_REPRODUCTION_THRESHOLD / repro_bias;
            if insect.energy > adjusted_threshold {
                // Use the pre-computed repro_budget; only allow offspring up to the cap.
                if new_insects.len() < repro_budget {
                    let lease_req = SpatialLease {
                        primary: insect.chunk,
                        fringe: vec![],
                        intent: LeaseIntent {
                            reads: vec![],
                            writes: vec![TypeId::of::<Tier4ReproduceWrite>()],
                        },
                        granted_at_causal_seq: current_seq,
                    };
                    match charter.request_lease(lease_req, current_seq) {
                        Ok(handle) => {
                            insect.energy -= T4_REPRODUCTION_COST;
                            insect.repro_urgency = 0.0;
                            insect.stw[2] = 0.5; // reproduction channel
                            // Offspring spawns at adjacent cell.
                            let off_x =
                                (insect.chunk.0 + 1).min(T4_CHUNK_EXTENT);
                            new_insects.push(Insect::new(ChunkId(off_x, insect.chunk.1), 2.0));
                            charter.release_lease(handle);
                            repro_grants += 1;
                        }
                        Err(_) => {
                            repro_denials += 1;
                        }
                    }
                } else {
                    repro_denials += 1;
                }
            }
        }

        survivors.push(insect);
    }

    // ── Decomposition writes (Tier 6 fungal link) ───────────────────────
    for chunk in dead_chunks {
        let lease_req = SpatialLease {
            primary: chunk,
            fringe: vec![],
            intent: LeaseIntent {
                reads: vec![],
                writes: vec![TypeId::of::<Tier4DecompWrite>()],
            },
            granted_at_causal_seq: current_seq,
        };
        match charter.request_lease(lease_req, current_seq) {
            Ok(handle) => {
                let existing = *chemistry_noise_floor.get(&chunk).unwrap_or(&0.0); // T4_DECOMP_DEFAULT
                let new_val = (existing + T4_DECOMP_DEPOSIT).min(T4_DECOMP_MAX);
                chemistry_noise_floor.insert(chunk, new_val);
                state.decomp_total += 1;
                charter.release_lease(handle);
            }
            Err(_) => {
                // Denied — no decomposition trace this tick; not a silent fallback,
                // the nutrient is simply lost (ecologically valid: scavengers ate it first).
            }
        }
    }

    // Commit survivors + offspring.
    survivors.extend(new_insects);
    state.insects = survivors;

    let population = state.insects.len();
    let avg_energy = if population == 0 {
        0.0
    } else {
        state.insects.iter().map(|i| i.energy).sum::<f32>() / population as f32
    };

    let metrics = Tier4TickMetrics {
        tick: current_seq,
        population,
        eat_grants,
        eat_denials,
        repro_grants,
        repro_denials,
        deaths,
        avg_energy,
    };

    state.metrics.push(metrics.clone());
    if state.metrics.len() > 600 {
        state.metrics.remove(0);
    }

    Some(metrics)
}

// ──────────────────────────────────────────────────────────────────────────
// Helper: population growth curve summary
// ──────────────────────────────────────────────────────────────────────────

/// Returns a brief multi-line string summarising the population growth curve.
/// Samples at 10%, 25%, 50%, 75%, 100% of the metrics history.
pub fn population_growth_summary(state: &Tier4State) -> String {
    if state.metrics.is_empty() {
        return "  Tier 4 population: no data".to_string();
    }
    let n = state.metrics.len();
    let indices = [0, n / 10, n / 4, n / 2, 3 * n / 4, n - 1];
    let mut lines = Vec::new();
    for i in indices {
        let m = &state.metrics[i.min(n - 1)];
        lines.push(format!(
            "    tick {:>8}: pop={:>5}  avg_energy={:.2}  eat_grants={}  repro_grants={}  deaths={}",
            m.tick, m.population, m.avg_energy, m.eat_grants, m.repro_grants, m.deaths
        ));
    }
    lines.join("\n")
}

// ──────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn bare_charter() -> SpatialCharter {
        SpatialCharter::default()
    }

    #[test]
    fn test_seed_produces_initial_population() {
        let mut state = Tier4State::default();
        seed_tier4_population(&mut state, 0);
        assert!(
            state.insects.len() >= T4_INITIAL_SEEDS / 2,
            "seed should produce close to T4_INITIAL_SEEDS insects, got {}",
            state.insects.len()
        );
        assert!(state.seeded);
    }

    #[test]
    fn test_advance_throttles() {
        let mut state = Tier4State::default();
        let mut grass: HashMap<ChunkId, u32> = HashMap::new();
        let mut chem: HashMap<ChunkId, f32> = HashMap::new();
        let mut charter = bare_charter();

        seed_tier4_population(&mut state, 0);
        // tick=1 should be throttled (interval is T4_UPDATE_INTERVAL_TICKS=100).
        let result = advance_tier4(1, &mut grass, &mut chem, None, &mut charter, &mut state);
        assert!(result.is_none(), "advance at tick=1 should be throttled");
    }

    #[test]
    fn test_advance_runs_at_interval() {
        let mut state = Tier4State::default();
        let mut grass: HashMap<ChunkId, u32> = HashMap::new();
        let mut chem: HashMap<ChunkId, f32> = HashMap::new();
        let mut charter = bare_charter();

        // Seed grass to allow eat grants.
        grass.insert(ChunkId(128, 128), 5);

        seed_tier4_population(&mut state, 0);
        let result = advance_tier4(
            T4_UPDATE_INTERVAL_TICKS,
            &mut grass,
            &mut chem,
            None,
            &mut charter,
            &mut state,
        );
        assert!(result.is_some(), "advance at T4_UPDATE_INTERVAL_TICKS should run");
        let m = result.expect("metrics must be present after advance at interval");
        assert!(m.population > 0);
    }

    #[test]
    fn test_population_cap() {
        let mut state = Tier4State::default();
        // Manually pack to just below cap.
        for i in 0..T4_MAX_POPULATION {
            let x = (i % 256) as u32;
            let y = (i / 256) as u32;
            state.insects.push(Insect::new(ChunkId(x, y), 6.0));
        }
        state.seeded = true;
        let mut grass: HashMap<ChunkId, u32> = HashMap::new();
        let mut chem: HashMap<ChunkId, f32> = HashMap::new();
        let mut charter = bare_charter();

        let result = advance_tier4(
            T4_UPDATE_INTERVAL_TICKS,
            &mut grass,
            &mut chem,
            None,
            &mut charter,
            &mut state,
        );
        assert!(result.is_some());
        let m = result.expect("metrics must be present");
        // Some deaths from metabolic cost will bring pop slightly below cap even with rich energy.
        // The key invariant: reproduction should be denied at cap.
        assert!(m.population <= T4_MAX_POPULATION, "population must not exceed cap");
    }

    #[test]
    fn test_charter_mediation_eat() {
        let mut state = Tier4State::default();
        state.insects.push(Insect::new(ChunkId(5, 5), 1.0));
        state.seeded = true;
        let mut grass: HashMap<ChunkId, u32> = HashMap::new();
        grass.insert(ChunkId(5, 5), 10);
        let mut chem: HashMap<ChunkId, f32> = HashMap::new();
        let mut charter = bare_charter();

        // Hold a conflicting write lease on the same chunk before advancing.
        let blocker = SpatialLease {
            primary: ChunkId(5, 5),
            fringe: vec![],
            intent: LeaseIntent {
                reads: vec![],
                writes: vec![TypeId::of::<Tier4GrassConsumeWrite>()],
            },
            granted_at_causal_seq: 0,
        };
        let handle = charter
            .request_lease(blocker, 0)
            .expect("blocker lease must be granted");

        let result = advance_tier4(
            T4_UPDATE_INTERVAL_TICKS,
            &mut grass,
            &mut chem,
            None,
            &mut charter,
            &mut state,
        );
        let m = result.expect("advance must run");
        // The eat should be denied due to the held lease.
        assert_eq!(m.eat_grants, 0, "eat should be denied when chunk is held");
        assert_eq!(m.eat_denials, 1, "eat_denials should record one denial");

        charter.release_lease(handle);
    }

    #[test]
    fn test_decomp_writes_chemistry() {
        let mut state = Tier4State::default();
        // One starving insect — it will die this tick.
        let mut dying = Insect::new(ChunkId(10, 10), T4_METABOLIC_COST * 0.5);
        dying.energy = T4_METABOLIC_COST * 0.5; // dies after metabolic deduction
        state.insects.push(dying);
        state.seeded = true;
        let mut grass: HashMap<ChunkId, u32> = HashMap::new();
        let mut chem: HashMap<ChunkId, f32> = HashMap::new();
        let mut charter = bare_charter();

        let result = advance_tier4(
            T4_UPDATE_INTERVAL_TICKS,
            &mut grass,
            &mut chem,
            None,
            &mut charter,
            &mut state,
        );
        let m = result.expect("advance must run");
        assert_eq!(m.deaths, 1, "one insect should have died");
        // Decomposition should have deposited something in chemistry.
        let dep = chem.get(&ChunkId(10, 10)).copied().unwrap_or(0.0);
        assert!(dep > 0.0, "decomp deposit must be non-zero after insect death");
    }
}
