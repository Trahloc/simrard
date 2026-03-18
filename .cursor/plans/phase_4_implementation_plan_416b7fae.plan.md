---
name: Phase 4 Implementation Plan
overview: Rewrite ImplementationPlan.md with detailed, checkbox-driven Phase 4 directives organized into sequential sub-phases that a smaller AI agent can follow step by step.
todos:
  - id: rewrite-impl-plan
    content: Rewrite ImplementationPlan.md with all Phase 4 sub-phases broken into detailed checkboxes
    status: completed
isProject: false
---

# Phase 4 Implementation Plan Rewrite

## Context

The current `ImplementationPlan.md` has a brief 4-line Phase 4 section. The user wants it rewritten into exhaustive, checkbox-level directives a smaller AI can execute without needing to re-derive context from the design docs.

## Key Findings from Code Analysis

The current simulation has critical gaps that make it feel lifeless:

- **No goal-directed movement**: `pawn_wander_system` moves pawns randomly (1-in-5 chance per tick). Pawns never walk toward food/water.
- **Actions require co-location**: `eat_action_system` / `drink_action_system` only find resources on `pawn_pos.chunk == food_pos.chunk`. If a pawn wanders away from food, it can never eat.
- **Resources permanently deplete**: 3 food portions + 2 water per cluster. Once consumed, food/water entities are despawned. Sim stalls.
- **Quest board is write-only**: Needs are posted but no pawn ever accepts or fulfills a quest. `QuestStatus::InProgress` is never set.
- **No discovery mechanic**: `run_curiosity_step` increments curiosity and posts "Learn Fire" quests, but nothing happens with them.
- **No knowledge graph**: Pawns have no memory of discoveries or recipes.
- **Scheduler debt**: Dispatcher directly mutates `NeuralNetworkComponent` drive values.

## Plan Structure

The new `ImplementationPlan.md` will be organized into these sub-phases, ordered by dependency:

1. **Phase 4.0 -- Goal-Directed Movement** (foundation for everything else)
2. **Phase 4.1 -- Resource Sustainability** (sim must not stall)
3. **Phase 4.2 -- Quest Acceptance & Economy** (pawns respond to needs)
4. **Phase 4.3 -- Thinker Improvements** (oscillation fix, scheduler debt)
5. **Phase 4.4 -- Recipe Discovery & Knowledge** (the deferred Phase 0 metric)
6. **Phase 4.5 -- Visualization & UI Polish** (feedback for observer mode)
7. **Phase 4.6 -- SimLife Sub-Simulation** (first Maxis layer)
8. **Phase 4.7 -- GPU NN & LLM Sidecar** (deferred, placeholder)
9. **Phase 4.8 -- Multiplayer** (deferred, placeholder)

Each sub-phase will include:

- Rationale (why this matters)
- Files to modify/create
- Detailed checkboxes with specific code guidance
- Verification steps
- What to NOT do (guardrails for the smaller AI)

## Key Architectural Decisions to Encode

- **MoveToChunk action**: New action + scorer pair. Pawn picks a target chunk (where the resource is), walks one chunk per sim tick toward it. Uses Chebyshev movement (matching the distance metric). Charter lease is only needed at the destination chunk, not during transit.
- **Resource regeneration**: `FoodReservation` and `WaterSource` get a `regeneration_rate` field. A system periodically adds portions back. Alternatively, respawn the entity after N ticks. Keep it simple -- the design doc says resources are substrate, not scripted.
- **Quest lifecycle**: A new system `quest_acceptance_system` runs each sim tick. Pawns with matching capabilities and high enough drives accept open quests. Accepted quest sets `QuestStatus::InProgress { provider }`. Fulfilled quests get cleaned up.
- **Knowledge graph**: Per-pawn `KnowledgeGraph` component. Start minimal: `HashSet<RecipeId>`. Knowledge propagation uses `CausalEventQueue` with `propagation_delay`.
- **Fire discovery**: When curiosity threshold crossed AND pawn is near specific resources, discovery fires. Teaching = social drive + proximity + causal propagation.

## Files to Change

Primary files:

- `[ImplementationPlan.md](ImplementationPlan.md)` -- complete rewrite
- References to: `[bin/src/simrard.rs](bin/src/simrard.rs)`, `[lib/ai/ai.rs](lib/ai/ai.rs)`, `[lib/pawn/pawn.rs](lib/pawn/pawn.rs)`, `[lib/causal/causal.rs](lib/causal/causal.rs)`, `[lib/causal/heartbeat.rs](lib/causal/heartbeat.rs)`, `[TODO.md](TODO.md)`

