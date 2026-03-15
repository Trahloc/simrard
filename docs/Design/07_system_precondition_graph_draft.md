# The System Precondition Graph Draft (TS1)

Status: Draft v0
Date: 2026-03-15
Scope: Neolithic to early-industrial legibility transitions.

## Purpose

Define when The System makes new categories legible (for example, "rock" -> "iron ore")
using explicit preconditions based on:
- world-state
- agent-history

This keeps progression discovered, not granted.

## Model

A precondition node unlocks when all of the following are true:
- World predicates: measurable environmental/resource state.
- Agent predicates: observed colony behavior or accumulated know-how.
- Stability window: predicates remain true for N ticks.

Each node emits:
- `LegibilityUnlock(node_id)`
- New allowed quest-board need/capability categories.

## Stability Rule

A node unlock is considered stable if predicates hold for `N=200` causal ticks.
This avoids flapping from transient spikes.

## Draft Node Set

### N1: Controlled Fire

Unlock effect:
- Enable `fire` recipe legibility.
- Enable quest-board categories: `ignite`, `maintain_fire`.

World predicates:
- At least one chunk has both food and rest activity in recent window.

Agent predicates:
- At least two unique pawns have `KnownRecipes` containing `Fire`.

### N2: Charred Cooking

Unlock effect:
- Enable `cook_food` recipe legibility.
- Enable quest-board category: `cook`.

World predicates:
- Fire-capable chunk exists for stability window.

Agent predicates:
- At least one pawn has completed `maintain_fire` behavior >= 3 times.
- At least one pawn has consumed food in a fire-capable chunk.

### N3: Basic Ore Awareness

Unlock effect:
- Enable `ore_identification` legibility category.
- Quest-board category: `survey_ore`.

World predicates:
- Colony has explored at least K unique chunks (initial draft K=30).

Agent predicates:
- At least one pawn with `Curiosity` episodes >= threshold and `cook_food` known.

### N4: Primitive Smelting

Unlock effect:
- Enable `smelt_ore` recipe legibility.
- Quest-board categories: `gather_fuel`, `smelt_batch`.

World predicates:
- Ore-aware chunks known.
- Sustained fire capacity over stability window.

Agent predicates:
- At least one pawn has `ore_identification` and `cook_food`.
- Colony has completed `gather_fuel`-type work >= M times (initial draft M=10).

### N5: Basic Tool Metalwork

Unlock effect:
- Enable `forge_basic_tool` legibility.
- Quest-board category: `forge_tool`.

World predicates:
- Smelting output has been produced in at least one chunk.

Agent predicates:
- At least one pawn has smelting history entries >= T (initial draft T=5).

## Predicate Encoding Draft

Represent each node as:
- `id: String`
- `world_predicates: Vec<Predicate>`
- `agent_predicates: Vec<Predicate>`
- `stability_ticks: u64`
- `unlock_effects: Vec<UnlockEffect>`

Predicate examples:
- `UniqueChunksVisited >= K`
- `KnownRecipeCount("Fire") >= 2`
- `ActionCount("maintain_fire") >= 3`

## Sequencing Notes

- N1 is expected to follow current Phase 4.4 discovery mechanics.
- N2 and N3 can unlock in either order depending on colony behavior.
- N4 depends on both sustained fire and ore awareness.
- N5 depends on repeated smelting history.

## Implementation Follow-ups

1. Add telemetry counters required by predicates.
2. Add a lightweight evaluator that checks predicates each sim tick.
3. Add event emission to QuestBoard category registry on unlock.
4. Add UI debug line for active/near-unlock nodes.
