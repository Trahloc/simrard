# Simrard Implementation Plan & Handoff Document

This document summarizes the technical state of Simrard and provides an implementation roadmap for the next agent.

**Layout**: simrard uses the r2026t workspace (`bin/`, `lib/*`, `tests/`). See `docs/rust-2026-trahloc.md`. Utility AI lives in `lib/utility_ai` (and `lib/utility_ai/derive`); consumed by `lib/ai` and `bin`.

## Current Technical State

### 1. Causal Layer (Foundation)
- **Transforms (`lib/transforms/`)**: Transactional transform system with schema validation (`validation.rs`) and hot-reloading (`hot_reload.rs`).
- **Time (`lib/time/`)**: `CausalClock` trait; `GlobalTickClock` for heartbeat and environment.
- **Causal (`lib/causal/`)**: 
    - `CausalEventQueue`, `CausalPropagationClock` (spatial propagation, C=8), Chebyshev distance and `propagation_delay`; all verified by tests.
    - Heartbeat system (`heartbeat.rs`) decays pawn drives and emits `DriveThresholdCrossed` events.

### 2. Coordination Layer
- **Spatial Charter (`lib/charter/`)**: `SpatialCharter`, `SpatialLease`, Read/Write intent; `test_exactly_one_pawn_gets_contested_food` proves concurrent exclusion.
- **Watchguard (`lib/charter/watchguard.rs`)**: `FrameWriteLog`; action systems log before release; watchguard verifies chunk and intent.

### 3. AI & Pawn Layer
- **Cognition**: In-house utility AI in `lib/utility_ai` (absorbed from big-brain, Bevy 0.17→0.18). Hunger/Thirst/Fatigue scorers and actions; `WaterSource`, `RestSpot`; heartbeat + dispatcher post needs to QuestBoard.
- **QuestBoard**, Item Identity (`ItemId`, `ItemHistory`), lease integration (Eat/Drink/Rest request and release leases).
- **Bevy**: All crates use Bevy 0.18. Charter flash uses Message API (`MessageWriter`/`MessageReader`, `Messages<CharterFlashEvent>`).

## Known Debt & Issues

### #scheduler-debt
- In `lib/ai/ai.rs`, the dispatcher mutates `NeuralNetworkComponent` drive values to force big-brain re-evaluation. Phase 4 goal: event-reactive big-brain integration when scheduler surface is matured.

### Warnings
- Warnings are errors (`-D warnings`). Unused imports and dead code have been cleared as of the r2026t conversion.

### Resolved (Post–Phase 4.1)
- **Scorer entity vs actor**: Utility AI scorer entities are children with only `Actor(pawn)` and `Score`; pawn state is on the actor. All four scorers in `lib/ai/ai.rs` were updated to query by `Actor` and look up the actor's `NeuralNetworkComponent` or `Position`/`MovementTarget`. This fixed headless runs (scores were previously always 0, so Thinker never chose Eat/Drink/Rest).
- **Score range**: Scorer outputs are clamped to `[0.0, 1.0]` so headed mode does not panic when needs exceed 1.0 (e.g. after eating). Headless run `--headless-test` now reaches 10k ticks with all pawns alive and consumption confirmed.

## Phase 3.5: Minimal Bevy 2D Visualizer (COMPLETED)

Scope (implemented in `bin/src/simrard.rs`, `lib/charter`, `lib/ai`):

1. **Chunk grid** — `chunk_grid_gizmo_system` draws 2D gizmo lines for chunk boundaries (extent 0..=11).
2. **Pawns** — colored circles via `pawn_dominant_drive_color_system`; color by dominant drive (hunger=red, thirst=blue, fatigue=gray); `PawnVisual` marker on pawns.
3. **Food/water sources** — rendered as colored sprites in setup (orange food, blue water, gray rest).
4. **Charter feedback** — `CharterFlashEvent` (Bevy 0.18 `Message`); AI action systems write via `MessageWriter`; `charter_flash_spawn_system` / `charter_flash_tick_system` spawn short-lived overlay sprites (green grant, red deny, ~0.2s).
5. **UI panel** — `setup_quest_ui` spawns top-left panel with four sections; `ui_panel_update_system` updates: **Sim status** (tick, speed, pause, key hints), **Legend** (pawn color = dominant need), **Quests** (from `QuestBoard`), **Activity** (last 8 entries from `ActivityLog`).
6. **Position ↔ visual** — `sync_position_to_transform` runs after `sim_tick_driver` and `pawn_wander_system`; pawns use `DisplayOffset` so multiple per chunk don't stack. Demo movement: `pawn_wander_system` steps some pawns to adjacent chunks each sim tick.
7. **Activity log** — `ActivityLog` resource (optional in `lib/ai`); dispatcher and action systems push short strings; bin inits it and displays in the Activity section.

**Rationale**: Watching the simulation answers questions logs cannot — clustering, C=8 feel, charter contention. The observer-mode proof is easier to evaluate by watching the screen.

---

## Phase 4: Sub-Phases (Checkbox-Driven)

Execute sub-phases in order. Each checkbox is a concrete task an AI agent can complete. Do not skip a sub-phase; dependencies are intentional.

---

### Phase 4.0 — Goal-Directed Movement

**Rationale**: Pawns currently wander randomly. Eat/Drink actions only succeed when `pawn_pos.chunk == food_pos.chunk`. Pawns must move toward resources to satisfy needs; otherwise the sim feels broken and resources go unused or unreachable.

**Files to modify/create**:
- `lib/pawn/pawn.rs` — add movement target component.
- `lib/ai/ai.rs` — add MoveToChunk action + scorer, wire into Thinker.
- `bin/src/simrard.rs` — remove or gate demo `pawn_wander_system`; ensure movement runs in sim tick.

**Checkboxes**:

- [x] **4.0.1** In `lib/pawn/pawn.rs`, add a component `MovementTarget(pub ChunkId)`. Optional: `Option<MovementTarget>`. When present, the pawn is trying to reach that chunk. Document that charter lease is only required at destination, not during transit.
- [x] **4.0.2** In `lib/ai/ai.rs`, add `MoveToChunkAction` (ActionBuilder) and a scorer that fires when the pawn has a `MovementTarget` and is not yet on that chunk (e.g. `NeedsToMoveScorer` that scores high when `position.chunk != target.0`). Register the scorer and action in `PawnAIPlugin` (PreUpdate, BigBrainSet::Scorers / Actions).
- [x] **4.0.3** Implement `move_to_chunk_action_system`. In `ActionState::Requested`: ensure the actor has `MovementTarget`; if so, set state to `Executing`. In `ActionState::Executing`: read `Position` and `MovementTarget`; compute one step toward target using Chebyshev movement (move in x, y, or both by at most 1 chunk so distance decreases). Use `simrard_lib_causal::chebyshev_distance` and move in the direction that reduces distance. Update `Position.chunk` by one step. If `position.chunk == target.0`, set state to `Success` and remove `MovementTarget`. Run this system in the same schedule as other actions (PreUpdate, BigBrainSet::Actions).
- [x] **4.0.4** Ensure Eat/Drink actions set a movement target when the pawn is not on the resource chunk: before requesting a lease, if `pawn_pos.chunk != food_pos.chunk`, insert `MovementTarget(food_pos.chunk)` and set action state to something that allows the thinker to re-select MoveToChunk (or keep action as "going to eat" but first step is moving). Alternative: add a composite flow — "Want to eat" scorer picks EatAction; EatAction in Requested, if not on chunk, insert MovementTarget and return without requesting lease; a separate MoveToChunk action runs until arrival, then next frame EatAction can request lease. Choose one approach and document it in a short comment.
- [x] **4.0.5** In `bin/src/simrard.rs`, remove `pawn_wander_system` from the Update chain, or gate it behind a config so default is goal-directed movement only. Ensure `sync_position_to_transform` still runs after the sim tick (and after any movement that happens during PreUpdate from big-brain actions). Confirm that movement is applied in the same frame as sim tick: PreUpdate runs before Update, and sim_tick_driver runs in Update; so either run movement in a system that runs after sim_tick_driver in Update, or run sim_tick_driver in PreUpdate before BigBrain. Check current order: sim_tick_driver runs in Update; BigBrain runs in PreUpdate. So BigBrain actions run the frame *before* the next sim tick. To avoid off-by-one, either (a) run MoveToChunk in Update after sim_tick_driver and have it read a "pending target" set by actions, or (b) run sim_tick_driver in PreUpdate before BigBrain so that each frame: tick, then scorers/thinker/actions (including move). Document the chosen ordering in a comment.
- [x] **4.0.6** Add a test or manual verification: spawn one pawn at (0,0), one food at (3,0); run sim; pawn should move (1,0), (2,0), (3,0) and then eat when on the same chunk.

**Verification**:
- `cargo build --workspace` and `cargo test --workspace` pass.
- Run `cargo run -p simrard-bin`; pawns should move toward food/water when hungry/thirsty and then eat/drink when on the same chunk.
- Run `cargo run -p simrard-bin -- --headless-test`; run reaches 10k ticks with all pawns alive (scorer fix ensures Eat/Drink/Rest are chosen).

**Do NOT**:
- Do not require a charter lease for the path; only the destination chunk needs a lease for the actual Eat/Drink.
- Do not use Euclidean distance for grid movement; use Chebyshev (already in `lib/causal`).

---

### Phase 4.1 — Resource Sustainability

**Rationale**: Food and water entities are despawned when portions hit zero. With no respawn, the sim eventually has no resources and pawns starve. The sim must not irreversibly stall. **Design choice**: Food and water **despawn** when depleted and **respawn at random chunks**; they do **not** share the same chunk (each chunk has at most one resource). **Stakes**: Pawns must move to food/water or they die — hunger or thirst at 0 causes despawn (death is failure).

**Files to modify/create**:
- `lib/pawn/pawn.rs` — `FoodReservation` and `WaterSource` have only `portions: u32` (no in-place regen).
- `lib/ai/ai.rs` — eat/drink always despawn when portions hit zero.
- `bin/src/simrard.rs` — `resource_respawn_system`: when food or water count is below target, spawn at a random **empty** chunk (no food and no water there); deterministic from `causal_seq` so no rand crate.

**Checkboxes**:

- [x] **4.1.1** Add optional regeneration to resources. Option A: Add `regeneration_per_tick: Option<u32>` (and optionally `max_portions: u32`) to `FoodReservation` and `WaterSource`. When `Some(rate)`, each sim tick (or every N ticks) add `rate` portions up to `max_portions`. Option B: Add a resource `ResourceRegenConfig` and a system that, every K causal ticks, finds all `FoodReservation`/`WaterSource` with `portions == 0` and adds 1 portion (or respawns from a template). Choose one and implement.
- [x] **4.1.2** In the eat/drink action systems, when the last portion is consumed: instead of despawn, set `portions = 0` and keep the entity if regeneration is enabled; or despawn and have a separate "spawner" system re-create the entity at that chunk after a delay. Ensure the visualizer still shows the entity (e.g. grayed out when empty, or respawned with same Position).
- [x] **4.1.3** In `bin/src/simrard.rs` `setup()`, give at least one food and one water source a regeneration value (or register them with the regen system) so that over long runs resources never permanently disappear.
- [x] **4.1.4** Verify: run sim at high speed for 500+ ticks; at least one cluster should still have food and water available (or respawned).
- [x] **4.1.5** Pawn death: after each sim tick, despawn any pawn with `hunger <= 0` or `thirst <= 0`; optionally log to `ActivityLog` (e.g. "Pawn_X died (hunger/thirst zero)").

**Verification**:
- Build and test pass. Long run (e.g. 1000 sim ticks) does not leave all clusters empty with no way to recover.

**Do NOT**:
- Do not make regeneration depend on player input or non-causal state.
- Do not remove the "depleted" logic entirely; keep depletion, add refill.

---

### Phase 4.2 — Quest Acceptance & Economy

**Rationale**: The QuestBoard receives needs (Open quests) but no pawn ever accepts them. `QuestStatus::InProgress` is never set. The emergent economy requires drive-weighted selection of open quests by capability.

**Files to modify/create**:
- `lib/ai/ai.rs` — quest acceptance system; optionally extend dispatcher or add a system that runs each sim tick.
- `lib/pawn/pawn.rs` — possibly extend `Quest` or add a component linking a pawn to an accepted quest.

**Checkboxes**:

- [ ] **4.2.1** Add a system `quest_acceptance_system` that runs each sim tick (after heartbeat and dispatcher). It queries `QuestBoard`, finds quests with `QuestStatus::Open`, then for each open quest finds pawns that (a) have a matching capability (`Capabilities::has(need)` or similar — map "food" to "Eat", "water" to "Drink", "rest" to "Rest"), (b) are in a state where they could fulfill it (e.g. hunger high for "food"), and (c) are not already the requester. Select one pawn per quest (e.g. highest drive or first match) and set the quest's `status` to `QuestStatus::InProgress { provider: that_pawn }`. Optionally insert a component on the provider, e.g. `AcceptedQuest(QuestId)` or store the quest index. Document how "drive-weighted" is implemented (e.g. sort candidates by drive value for that need).
- [ ] **4.2.2** When a pawn completes an eat/drink/rest action that satisfies a quest (requester is the pawn or the quest need matches), mark that quest as `QuestStatus::Completed` and remove it from `active_quests` after a tick, or move to a "completed" list for UI. Ensure the accepting pawn (provider) is the one who performs the action; the requester is the one who had the need. Clarify in code: acceptance means "provider will go fulfill this need for the requester" or "provider will fulfill their own need that was posted as a quest". Current design: requester posts their own need; so the provider that accepts is actually going to fulfill the requester's need — that implies going to the requester's chunk or the resource. For simplicity, first implement: acceptance means "a pawn with matching capability commits to fulfilling this need"; fulfillment is when that pawn completes the action (e.g. eats at the chunk). So the quest's `chunk` is where the need was posted (requester's location) or where the resource is. Align with existing `Quest { need, requester, chunk, provider, status }`: when a pawn accepts, set `provider: Some(acceptor_entity)` and `status: InProgress`. When that pawn eats/drinks/rests at the relevant chunk, mark quest completed.
- [ ] **4.2.3** Limit one accepted quest per pawn if needed (e.g. component `AcceptedQuest(pub usize)` indexing into quest board, or store entity of the quest). Ensure the same quest is not accepted by two pawns (status is InProgress with one provider).
- [ ] **4.2.4** Expose accepted quests in the UI: in `ui_panel_update_system`, show which quests are Open vs InProgress (with provider name if available) vs Completed. Optionally trim "Completed" after N entries or one tick.
- [ ] **4.2.5** Add a unit test or integration test: post one quest, run acceptance system, assert one quest has InProgress and provider set (when at least one pawn has the capability and high drive).

**Verification**:
- Build and test pass. In-game, when a need is posted, within a few ticks a pawn should accept it (shown in UI) and then fulfill it (quest disappears or shows completed).

**Do NOT**:
- Do not allow two pawns to be the provider for the same quest.
- Do not remove the existing "post need to QuestBoard" logic in the dispatcher; extend it with acceptance.

---

### Phase 4.3 — Thinker Improvements

**Rationale**: (1) Pawns can oscillate between equally-scored actions (e.g. hunger and thirst both 0.8). (2) Dispatcher directly mutates drive values to force re-evaluation (#scheduler-debt). We improve hysteresis and document/shrink the scheduler debt.

**Files to modify/create**:
- `lib/utility_ai/src/thinker.rs` — hysteresis or tie-breaking.
- `lib/ai/ai.rs` — dispatcher mutation; optional event-based re-trigger.

**Checkboxes**:

- [ ] **4.3.1** Implement oscillation protection in the Thinker. In `lib/utility_ai/src/thinker.rs`, when the current action's score is within a small epsilon of another choice (e.g. difference < 0.05), prefer the current action (do not switch). Add a constant e.g. `HYSTERESIS_EPSILON: f32 = 0.05` and apply in the picker or in the thinker loop that selects the next action. Document: "Hysteresis: avoid flipping between equally-scored actions."
- [ ] **4.3.2** Optionally add a "cooldown" so that after finishing an action, the same action cannot be re-selected for N ticks (to avoid eat-eat-eat in place). If added, make N configurable (e.g. 3–5 ticks) and document.
- [ ] **4.3.3** #scheduler-debt: Document in `lib/ai/ai.rs` at the dispatcher site exactly why we mutate the drive (e.g. "Force re-evaluation so Thinker re-scores; TODO(YYYY-MM-DD): replace with event-reactive re-trigger when BigBrain supports it."). Do not remove the mutation until event-reactive integration exists; ensure one comment references #scheduler-debt.
- [ ] **4.3.4** Evaluate default Picker: in `lib/utility_ai/src/thinker.rs`, check whether `ThinkerBuilder` should provide a default `Picker` if none is set (see existing TODO around line 231). Either add a default (e.g. `FirstToScore { threshold: 0.8 }`) or document "Picker is required" and panic/assert if missing.

**Verification**:
- Build and test pass. In-game, pawns should not visibly flicker between eat/drink/rest every frame when two drives are close.

**Do NOT**:
- Do not change the external API of Thinker/ThinkerBuilder in a breaking way without updating all call sites (e.g. `build_pawn_brain()` in `lib/ai`).

---

### Phase 4.4 — Recipe Discovery & Knowledge

**Rationale**: Phase 0 success metric (curiosity → fire discovery → teaching → zone formation) was deferred. This sub-phase implements minimal recipe discovery and knowledge propagation so that emergent "learning" can be observed.

**Files to modify/create**:
- `lib/pawn/pawn.rs` — `KnowledgeGraph` or `KnownRecipes` component.
- `lib/causal/causal.rs` — optional event kind `DiscoveryPropagated` or similar.
- `lib/ai/ai.rs` — discovery step: when curiosity high and conditions met, add recipe to pawn knowledge; post teaching event with propagation delay.

**Checkboxes**:

- [ ] **4.4.1** Add a component `KnownRecipes` (e.g. `pub recipes: std::collections::HashSet<String>` or a newtype `RecipeId`). Add to pawns in `bin/src/simrard.rs` at spawn. Initially empty or with one starter recipe.
- [ ] **4.4.2** Define at least one "recipe" that can be discovered (e.g. "Fire" or "Cook"). In `run_curiosity_step` (or a dedicated `discovery_system`), when a pawn's curiosity is above a threshold and the pawn is on a chunk that has certain resources (e.g. food + rest spot), with some probability or deterministically add "Fire" to that pawn's `KnownRecipes`. Log to ActivityLog: "Pawn X discovered Fire."
- [ ] **4.4.3** Teaching: when pawn A has discovered a recipe and pawn B does not, and A and B are on the same chunk (or within 1 chunk), and social drive is high, emit a causal event `DiscoveryPropagated { recipe, from: A, to: B }` with `deliver_at = current_seq + propagation_delay(A.chunk, B.chunk, C)`. In the dispatcher (or a new handler), when this event is ready, add the recipe to B's `KnownRecipes`. Log: "Pawn B learned Fire from Pawn A."
- [ ] **4.4.4** Add `CausalEventKind::DiscoveryPropagated { recipe: String, from: Entity, to: Entity }` in `lib/causal/causal.rs`. Extend `pawn_event_dispatcher_step` (or a separate system that reads the queue) to handle it and update `KnownRecipes` for the `to` entity.
- [ ] **4.4.5** Zone formation: optional. If "Fire" is known, a pawn could post a quest or place a "campfire zone" marker. Defer to a single checkbox: "If time permits, add a placeholder Zone or Quest type for 'build campfire' when a pawn knows Fire; otherwise leave a TODO."
- [ ] **4.4.6** Expose discovery in UI: in the activity feed, show "X discovered Y" and "X learned Y from Z". Optionally show per-pawn known recipes in a debug panel.

**Verification**:
- Build and test pass. Over a long run, at least one pawn discovers the recipe and at least one other pawn can learn it via proximity (teaching event).

**Do NOT**:
- Do not block discovery on LLM or GPU; keep it deterministic or simple RNG for now.
- Do not add more than 1–2 recipes in this sub-phase; focus on the pipeline.

---

### Phase 4.5 — Visualization & UI Polish

**Rationale**: Observer mode is easier to evaluate when the player can see movement trails, resource levels, and clearer feedback. This sub-phase improves the visualizer without changing sim logic.

**Files to modify/create**:
- `bin/src/simrard.rs` — UI and gizmo additions.

**Checkboxes**:

- [ ] **4.5.1** Show resource levels on food/water entities: if `FoodReservation` or `WaterSource` has `portions`, render a small text label above the sprite (e.g. "3") or a bar. Use Bevy UI or `Text2d`/gizmo; keep it minimal.
- [ ] **4.5.2** Optionally show pawn name or a compact ID on hover or as a small label (configurable or behind a debug flag) so that activity log lines can be matched to on-screen pawns.
- [ ] **4.5.3** Camera: add simple pan (e.g. arrow keys or middle-drag) and optional zoom so large grids are navigable. If not already present, ensure the camera covers the chunk grid (0..=11) by default.
- [ ] **4.5.4** Speed and pause keys: already R / [ ] / P. Ensure the UI panel text reflects current keys and shows speed with 2 decimal places or integer when large. No functional change if already done.
- [ ] **4.5.5** Optional: brief movement trail (e.g. last 3–5 chunk positions as dim dots) for one "selected" pawn to make pathfinding visible. Defer if time-constrained with a TODO.

**Verification**:
- Build and run; UI is readable and resources show state. Camera can pan if implemented.

**Do NOT**:
- Do not add gameplay logic in the visualizer; only display existing state.
- Do not break existing charter flash or activity feed.

---

### Phase 4.6 — SimLife Sub-Simulation (First Maxis Layer)

**Rationale**: Design doc (05_simulation_systems.md) describes sub-layers (SimEarth, SimLife, SimAnt). SimLife (ecology, food webs) drives surface-layer pressure with relatively low implementation cost. This sub-phase adds a minimal SimLife layer that influences surface resources.

**Files to modify/create**:
- New crate or module: e.g. `lib/simlife` or `lib/ecology`; or a module inside `lib/ai` or `bin` for the first iteration.
- Surface integration: something that reads SimLife output and adjusts food/water availability or spawns.

**Checkboxes**:

- [ ] **4.6.1** Create a minimal SimLife model: e.g. a 2D grid of "biome" or "population" counts (grass, prey, predator). No need for full ecology; just a placeholder that updates each sim tick (e.g. grass += 1 every 10 ticks in some chunks, capped). Document: "SimLife placeholder: provides pressure signals for surface layer."
- [ ] **4.6.2** Expose a read-only view: e.g. a resource `SimLifeState { grass_per_chunk: HashMap<ChunkId, u32> }` or similar that the surface simulation can query. Surface logic: e.g. "food regeneration rate in a chunk is proportional to SimLife grass there." Wire one surface mechanic to SimLife (e.g. food regen in `lib/pawn` or `lib/ai` reads `SimLifeState`).
- [ ] **4.6.3** Run SimLife step in the same sim tick (e.g. after heartbeat, before or after dispatcher). Ensure causal ordering is documented (SimLife runs at current_seq, surface reads it in the same tick).
- [ ] **4.6.4** Add a test that SimLife state advances (e.g. grass increases over ticks) and that surface food regen (if wired) is non-zero when grass > 0.

**Verification**:
- Build and test pass. SimLife runs and at least one surface mechanic (e.g. food regen) depends on it.

**Do NOT**:
- Do not implement full food webs or complex ecology in this sub-phase; keep it a stub that can be expanded later.
- Do not make SimLife depend on pawn actions for its tick; it is environmental.

---

### Phase 4.7 — GPU NN & LLM Sidecar (Deferred Placeholder)

**Rationale**: Scale to 64–256 drives and optional LLM for speech/prefrontal layer are deferred. This section is a checklist for when the team is ready.

**Checkboxes** (all deferred; do not implement until Phase 4.0–4.6 are done):

- [ ] **4.7.1** Expose `NeuralNetworkComponent` as a hot-reloadable transform (see design doc). Document in transforms schema.
- [ ] **4.7.2** GPU NN: integrate wgpu or Torch for 64–256 drive inference; keep 8-drive CPU path as fallback. Document budget: 4 GB per colony.
- [ ] **4.7.3** LLM sidecar: optional. Bevy communicates with a local async socket (Python or Rust/candle). Inference request/response non-blocking. Structured output: speech text + valence + drive signals. Document in ImplementationPlan or a new doc.

**Verification**: N/A until implemented.

**Do NOT**: Do not start 4.7 until 4.0–4.6 are complete and verified.

---

### Phase 4.8 — Multiplayer (Deferred Placeholder)

**Rationale**: Multiplayer is a separate problem with separate tradeoffs. Architecture should not block it, but implementation is deferred.

**Checkboxes** (deferred):

- [ ] **4.8.1** Document that causal events and charter leases are serializable and that no global tick is required; multiplayer can reuse the same causal model.
- [ ] **4.8.2** No implementation in Phase 4; leave as future work.

**Verification**: N/A.

---

## Verification (Workspace-Wide)

- `cargo build --workspace` — build all crates (Bevy 0.18, `lib/utility_ai` in workspace).
- `cargo test --workspace` — unit tests (causal, charter, transforms, integration stub).
- `cargo run -p simrard-bin` — run the binary (from repo root so `transforms/` is found).

After each Phase 4 sub-phase, re-run the above and the sub-phase-specific verification steps before proceeding.
