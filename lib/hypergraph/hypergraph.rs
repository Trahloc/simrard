use bevy::prelude::{Resource, UVec2};
use rand::{rngs::SmallRng, Rng, SeedableRng};
use ron::de::from_str;
use serde::{Deserialize, Serialize};

const MIX64: u64 = 0x9e37_79b9_7f4a_7c15;
const SUBSYSTEM_SALT: u64 = 0x517c_c1b7_2722_0a95;
const QUANTIZATION_STEPS: f32 = 1024.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PatchCoord {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionOutputs {
    pub density: f32,
    pub avg_arity: f32,
    pub clustering: f32,
    pub causal_volume: f32,
}

impl Default for RegionOutputs {
    fn default() -> Self {
        Self {
            density: 0.0,
            avg_arity: 0.0,
            clustering: 0.0,
            causal_volume: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewriteRule {
    pub name: String,
    pub pattern: Pattern,
    pub replacement: Replacement,
    pub probability: f32,
    pub bias: ArityBias,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pattern {
    Triangle,
    Line,
    Star,
    RandomPair,
    Cycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Replacement {
    AddNodeWithEdges { arity: u8 },
    Merge,
    Split,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArityBias {
    Low,
    High,
    Neutral,
}

#[derive(Debug, Clone, Copy)]
struct Node {
    id: u16,
    pos: UVec2,
    age: u8,
}

#[derive(Debug, Clone, Copy)]
struct Hyperedge {
    nodes: [u16; 4],
    arity: u8,
}

#[derive(Debug, Clone, Copy)]
struct PatchSnapshot {
    node_count: usize,
    edge_count: usize,
    avg_age: f32,
    output: RegionOutputs,
}

#[derive(Debug, Clone)]
struct Patch {
    coord: PatchCoord,
    next_node_id: u16,
    nodes: Vec<Node>,
    edges: Vec<Hyperedge>,
    output_cache: RegionOutputs,
}

impl Patch {
    fn seeded(coord: PatchCoord) -> Self {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for i in 0..4u16 {
            nodes.push(Node {
                id: i,
                pos: UVec2::new(i as u32 % 2, i as u32 / 2),
                age: (coord.x.wrapping_add(coord.y) as u8).saturating_add(i as u8),
            });
        }
        edges.push(Hyperedge {
            nodes: [0, 1, 2, 0],
            arity: 3,
        });
        Self {
            coord,
            next_node_id: 4,
            nodes,
            edges,
            output_cache: RegionOutputs::default(),
        }
    }

    fn snapshot(&self) -> PatchSnapshot {
        let avg_age = if self.nodes.is_empty() {
            0.0
        } else {
            self.nodes.iter().map(|n| n.age as f32).sum::<f32>() / self.nodes.len() as f32
        };
        PatchSnapshot {
            node_count: self.nodes.len(),
            edge_count: self.edges.len(),
            avg_age,
            output: self.output_cache,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HypergraphConfig {
    pub patch_cols: u32,
    pub patch_rows: u32,
    pub patch_chunk_size: u32,
    pub interval_ticks: u64,
    pub max_nodes_per_patch: usize,
    pub chaos: f32,
    pub ema_alpha: f32,
}

impl Default for HypergraphConfig {
    fn default() -> Self {
        Self {
            patch_cols: 32,
            patch_rows: 32,
            patch_chunk_size: 8,
            interval_ticks: 10_000,
            max_nodes_per_patch: 16,
            chaos: 0.15,
            ema_alpha: 0.12,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StepStats {
    pub considered: u32,
    pub denied: u32,
    pub rewritten: u32,
}

#[derive(Resource, Debug, Clone)]
pub struct HypergraphSubstrate {
    config: HypergraphConfig,
    patches: Vec<Patch>,
    rules: Vec<RewriteRule>,
    pub last_update_tick: u64,
}

impl Default for HypergraphSubstrate {
    fn default() -> Self {
        Self::new(HypergraphConfig::default(), default_rules())
    }
}

impl HypergraphSubstrate {
    pub fn new(config: HypergraphConfig, rules: Vec<RewriteRule>) -> Self {
        let mut patches = Vec::new();
        for y in 0..config.patch_rows {
            for x in 0..config.patch_cols {
                patches.push(Patch::seeded(PatchCoord { x, y }));
            }
        }

        Self {
            config,
            patches,
            rules,
            last_update_tick: 0,
        }
    }

    pub fn config(&self) -> HypergraphConfig {
        self.config
    }

    pub fn chaos(&self) -> f32 {
        self.config.chaos
    }

    pub fn set_chaos(&mut self, chaos: f32) {
        self.config.chaos = chaos.clamp(0.0, 1.0);
    }

    pub fn set_interval_ticks(&mut self, interval_ticks: u64) {
        self.config.interval_ticks = interval_ticks.max(1);
    }

    pub fn patch_dimensions(&self) -> (u32, u32) {
        (self.config.patch_cols, self.config.patch_rows)
    }

    pub fn patch_coords(&self) -> impl Iterator<Item = PatchCoord> + '_ {
        self.patches.iter().map(|p| p.coord)
    }

    pub fn patch_primary_chunk(&self, coord: PatchCoord) -> (u32, u32) {
        (
            coord.x.saturating_mul(self.config.patch_chunk_size),
            coord.y.saturating_mul(self.config.patch_chunk_size),
        )
    }

    pub fn output_for_chunk(&self, chunk_x: u32, chunk_y: u32) -> Option<RegionOutputs> {
        let px = chunk_x / self.config.patch_chunk_size;
        let py = chunk_y / self.config.patch_chunk_size;
        self.patch_output(PatchCoord { x: px, y: py })
    }

    pub fn patch_output(&self, coord: PatchCoord) -> Option<RegionOutputs> {
        self.patch_index(coord).map(|idx| self.patches[idx].output_cache)
    }

    pub fn step_with_permissions<F>(&mut self, causal_seq: u64, mut can_write_patch: F) -> StepStats
    where
        F: FnMut(PatchCoord) -> bool,
    {
        if self.config.interval_ticks == 0 || causal_seq % self.config.interval_ticks != 0 {
            return StepStats::default();
        }

        let snapshots: Vec<PatchSnapshot> = self.patches.iter().map(Patch::snapshot).collect();
        let mut stats = StepStats::default();

        for patch_idx in 0..self.patches.len() {
            let coord = self.patches[patch_idx].coord;
            stats.considered += 1;
            if !can_write_patch(coord) {
                stats.denied += 1;
                continue;
            }

            let neighbor = self.neighbor_snapshot(&snapshots, coord);
            let mut rng = seeded_rng(causal_seq, coord, self.config.chaos);
            let rule = self.pick_rule(&mut rng).cloned();

            let mut rewritten = false;
            if let Some(rule) = rule.as_ref() {
                let p_effective = (rule.probability * (0.5 + self.config.chaos)).clamp(0.0, 1.0);
                if rng.r#gen::<f32>() <= p_effective {
                    rewritten = apply_rewrite(
                        &mut self.patches[patch_idx],
                        rule,
                        neighbor,
                        self.config.max_nodes_per_patch,
                        &mut rng,
                    );
                }
            }

            if rewritten {
                stats.rewritten += 1;
            }

            let prev = self.patches[patch_idx].output_cache;
            let raw = compute_outputs(&self.patches[patch_idx], neighbor, self.config.max_nodes_per_patch as f32);
            self.patches[patch_idx].output_cache = smooth_and_quantize(prev, raw, self.config.ema_alpha);
        }

        self.last_update_tick = causal_seq;
        stats
    }

    fn patch_index(&self, coord: PatchCoord) -> Option<usize> {
        if coord.x >= self.config.patch_cols || coord.y >= self.config.patch_rows {
            return None;
        }
        Some((coord.y * self.config.patch_cols + coord.x) as usize)
    }

    fn neighbor_snapshot(&self, snapshots: &[PatchSnapshot], coord: PatchCoord) -> PatchSnapshot {
        let mut node_count = 0usize;
        let mut edge_count = 0usize;
        let mut avg_age_sum = 0.0;
        let mut density_sum = 0.0;
        let mut arity_sum = 0.0;
        let mut cluster_sum = 0.0;
        let mut causal_sum = 0.0;
        let mut samples = 0.0;

        for (dx, dy) in [(0i32, 0i32), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            let nx = coord.x as i32 + dx;
            let ny = coord.y as i32 + dy;
            if nx < 0 || ny < 0 {
                continue;
            }
            let ncoord = PatchCoord {
                x: nx as u32,
                y: ny as u32,
            };
            if let Some(idx) = self.patch_index(ncoord) {
                let snap = snapshots[idx];
                node_count += snap.node_count;
                edge_count += snap.edge_count;
                avg_age_sum += snap.avg_age;
                density_sum += snap.output.density;
                arity_sum += snap.output.avg_arity;
                cluster_sum += snap.output.clustering;
                causal_sum += snap.output.causal_volume;
                samples += 1.0;
            }
        }

        if samples == 0.0 {
            return PatchSnapshot {
                node_count: 0,
                edge_count: 0,
                avg_age: 0.0,
                output: RegionOutputs::default(),
            };
        }

        PatchSnapshot {
            node_count,
            edge_count,
            avg_age: avg_age_sum / samples,
            output: RegionOutputs {
                density: density_sum / samples,
                avg_arity: arity_sum / samples,
                clustering: cluster_sum / samples,
                causal_volume: causal_sum / samples,
            },
        }
    }

    fn pick_rule<'a>(&'a self, rng: &mut SmallRng) -> Option<&'a RewriteRule> {
        if self.rules.is_empty() {
            None
        } else {
            Some(&self.rules[rng.gen_range(0..self.rules.len())])
        }
    }
}

fn seeded_rng(causal_seq: u64, coord: PatchCoord, chaos: f32) -> SmallRng {
    let chaos_bits = (chaos.clamp(0.0, 1.0) * 1_000_000.0) as u64;
    let seed = causal_seq
        .wrapping_mul(MIX64)
        .wrapping_add((coord.x as u64).wrapping_shl(32))
        .wrapping_add(coord.y as u64)
        .wrapping_add(SUBSYSTEM_SALT)
        .wrapping_add(chaos_bits.wrapping_mul(131));
    SmallRng::seed_from_u64(seed)
}

fn apply_rewrite(
    patch: &mut Patch,
    rule: &RewriteRule,
    neighbor: PatchSnapshot,
    max_nodes_per_patch: usize,
    rng: &mut SmallRng,
) -> bool {
    match rule.replacement {
        Replacement::AddNodeWithEdges { arity } => {
            if patch.nodes.len() >= max_nodes_per_patch {
                return false;
            }
            let id = patch.next_node_id;
            patch.next_node_id = patch.next_node_id.saturating_add(1);
            let x_bias = if matches!(rule.bias, ArityBias::High) {
                (neighbor.node_count as u32) % 4
            } else {
                0
            };
            patch.nodes.push(Node {
                id,
                pos: UVec2::new((id as u32 + x_bias) % 4, ((id as u32) / 4) % 4),
                age: 0,
            });

            if patch.nodes.len() > 1 {
                let arity = arity.clamp(2, 4);
                let mut edge_nodes = [id; 4];
                for slot in edge_nodes.iter_mut().take(arity as usize) {
                    let pick = rng.gen_range(0..patch.nodes.len());
                    *slot = patch.nodes[pick].id;
                }
                patch.edges.push(Hyperedge {
                    nodes: edge_nodes,
                    arity,
                });
            }
            true
        }
        Replacement::Merge => {
            if patch.nodes.len() <= 2 {
                return false;
            }
            let remove_idx = rng.gen_range(0..patch.nodes.len());
            let removed = patch.nodes.remove(remove_idx).id;
            patch.edges.retain(|edge| !edge.nodes[..edge.arity as usize].contains(&removed));
            true
        }
        Replacement::Split => {
            if patch.edges.is_empty() {
                return false;
            }
            let edge_idx = rng.gen_range(0..patch.edges.len());
            let edge = patch.edges[edge_idx];
            if edge.arity < 3 {
                return false;
            }

            patch.edges[edge_idx].arity -= 1;
            let mut new_edge = edge;
            new_edge.arity = 2;
            patch.edges.push(new_edge);
            true
        }
    }
}

fn compute_outputs(patch: &Patch, neighbor: PatchSnapshot, max_nodes_per_patch: f32) -> RegionOutputs {
    let density = (patch.nodes.len() as f32 / max_nodes_per_patch).clamp(0.0, 1.0);
    let avg_arity = if patch.edges.is_empty() {
        0.0
    } else {
        patch.edges.iter().map(|e| e.arity as f32).sum::<f32>() / patch.edges.len() as f32
    };

    let avg_edge_to_node = (patch.edges.len() as f32 / patch.nodes.len().max(1) as f32).clamp(0.0, 1.0);
    let spatial_spread = if patch.nodes.is_empty() {
        0.0
    } else {
        let mean_x = patch.nodes.iter().map(|n| n.pos.x as f32).sum::<f32>() / patch.nodes.len() as f32;
        let mean_y = patch.nodes.iter().map(|n| n.pos.y as f32).sum::<f32>() / patch.nodes.len() as f32;
        let var = patch
            .nodes
            .iter()
            .map(|n| {
                let dx = n.pos.x as f32 - mean_x;
                let dy = n.pos.y as f32 - mean_y;
                dx * dx + dy * dy
            })
            .sum::<f32>()
            / patch.nodes.len() as f32;
        (var / 8.0).clamp(0.0, 1.0)
    };
    let clustering = (avg_edge_to_node * 0.55 + neighbor.output.density * 0.25 + spatial_spread * 0.2)
        .clamp(0.0, 1.0);

    let avg_age = if patch.nodes.is_empty() {
        0.0
    } else {
        patch.nodes.iter().map(|n| n.age as f32).sum::<f32>() / patch.nodes.len() as f32
    };
    let neighbor_age = (neighbor.avg_age / 255.0).clamp(0.0, 1.0);
    let causal_volume = ((avg_age / 255.0) * 0.6 + neighbor_age * 0.4).clamp(0.0, 1.0);

    RegionOutputs {
        density,
        avg_arity: (avg_arity / 4.0).clamp(0.0, 1.0),
        clustering,
        causal_volume,
    }
}

fn smooth_and_quantize(prev: RegionOutputs, raw: RegionOutputs, alpha: f32) -> RegionOutputs {
    let a = alpha.clamp(0.01, 1.0);
    RegionOutputs {
        density: quantize(prev.density + a * (raw.density - prev.density)),
        avg_arity: quantize(prev.avg_arity + a * (raw.avg_arity - prev.avg_arity)),
        clustering: quantize(prev.clustering + a * (raw.clustering - prev.clustering)),
        causal_volume: quantize(prev.causal_volume + a * (raw.causal_volume - prev.causal_volume)),
    }
}

fn quantize(value: f32) -> f32 {
    ((value.clamp(0.0, 1.0) * QUANTIZATION_STEPS).round() / QUANTIZATION_STEPS).clamp(0.0, 1.0)
}

fn default_rules() -> Vec<RewriteRule> {
    let source = include_str!("default_rules.ron");
    from_str(source).expect("hypergraph default_rules.ron invalid; fix lib/hypergraph/default_rules.ron")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn always_allow(_coord: PatchCoord) -> bool {
        true
    }

    #[test]
    fn deterministic_step_with_same_inputs() {
        let config = HypergraphConfig {
            patch_cols: 4,
            patch_rows: 4,
            patch_chunk_size: 8,
            interval_ticks: 10,
            max_nodes_per_patch: 12,
            chaos: 0.55,
            ema_alpha: 0.15,
        };
        let rules = default_rules();
        let mut left = HypergraphSubstrate::new(config, rules.clone());
        let mut right = HypergraphSubstrate::new(config, rules);

        for seq in 1..=40 {
            let left_stats = left.step_with_permissions(seq, always_allow);
            let right_stats = right.step_with_permissions(seq, always_allow);
            assert_eq!(left_stats, right_stats);
        }

        let left_out: Vec<_> = left.patch_coords().filter_map(|c| left.patch_output(c)).collect();
        let right_out: Vec<_> = right.patch_coords().filter_map(|c| right.patch_output(c)).collect();
        assert_eq!(left_out, right_out);
    }

    #[test]
    fn outputs_are_bounded_after_steps() {
        let mut substrate = HypergraphSubstrate::new(
            HypergraphConfig {
                patch_cols: 3,
                patch_rows: 3,
                interval_ticks: 1,
                ..Default::default()
            },
            default_rules(),
        );

        for seq in 1..=25 {
            let _ = substrate.step_with_permissions(seq, always_allow);
        }

        for coord in substrate.patch_coords() {
            let o = substrate.patch_output(coord).expect("patch output exists");
            assert!((0.0..=1.0).contains(&o.density));
            assert!((0.0..=1.0).contains(&o.avg_arity));
            assert!((0.0..=1.0).contains(&o.clustering));
            assert!((0.0..=1.0).contains(&o.causal_volume));
        }
    }

    #[test]
    fn empty_patch_paths_do_not_panic() {
        let mut substrate = HypergraphSubstrate::new(
            HypergraphConfig {
                patch_cols: 1,
                patch_rows: 1,
                interval_ticks: 1,
                ..Default::default()
            },
            vec![RewriteRule {
                name: "merge_only".to_string(),
                pattern: Pattern::Line,
                replacement: Replacement::Merge,
                probability: 1.0,
                bias: ArityBias::Neutral,
            }],
        );

        // force empty patch to exercise no-node/no-edge guards
        let patch = substrate.patches.first_mut().expect("patch exists");
        patch.nodes.clear();
        patch.edges.clear();

        for seq in 1..=3 {
            let _ = substrate.step_with_permissions(seq, always_allow);
        }

        let output = substrate.patch_output(PatchCoord { x: 0, y: 0 }).expect("output exists");
        assert!((0.0..=1.0).contains(&output.causal_volume));
    }

    #[test]
    fn neighbor_window_changes_output() {
        let config = HypergraphConfig {
            patch_cols: 2,
            patch_rows: 1,
            interval_ticks: 1,
            ..Default::default()
        };
        let rules = vec![RewriteRule {
            name: "stable".to_string(),
            pattern: Pattern::Line,
            replacement: Replacement::Split,
            probability: 0.0,
            bias: ArityBias::Neutral,
        }];
        let mut substrate = HypergraphSubstrate::new(config, rules);

        let left = substrate.patch_index(PatchCoord { x: 0, y: 0 }).expect("left patch");
        substrate.patches[left].output_cache.density = 1.0;

        let _ = substrate.step_with_permissions(1, always_allow);

        let right_out = substrate
            .patch_output(PatchCoord { x: 1, y: 0 })
            .expect("right output");
        assert!(right_out.clustering > 0.0);
    }
}
