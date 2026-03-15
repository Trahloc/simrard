# Simrard TODOs

This document tracks the high-level roadmap and specific tasks for Simrard, based on the original architecture and game design document.

**Layout**: r2026t workspace conversion completed (see `docs/rust-2026-trahloc.md`, `rust-r2026t.mdc`).

## 🟩 Phase 0: Prove Observer-Mode Drama (COMPLETED)
Phase 0 infrastructure COMPLETED. Full success metric (curiosity → fire discovery → teaching → zone formation) deferred to Phase 4.

- [x] Set up empty Cargo project and `Cargo.toml` (Bevy, big-brain, serde).
- [x] Implement minimal Transform schema (`transforms/schema.rs`).
- [x] Implement solidification and hot-reload logic.
- [x] Implement `main.rs` skeleton.
- [x] Implement 8-drive Neural Network component (Layer 1).
- [x] Implement declarative utility AI (Layer 2 - big-brain).
- [x] Implement emergent needs-based Quest Board.
- [x] Verify Observer mode drama over 10 sim-time hours with zero player input.

## 🟩 Phase 1: Full Transform Pipeline (COMPLETED)
- [x] Implement epoch micro-versioning system.
- [x] Implement contract validation (start simple, grow to SAT solver).
- [x] Extend `Transform` schema to cover generic component operations.
- [x] Build UI/CLI for generating and viewing active transform stacks. (Deferring CLI viewer to later GUI pass)
- [x] Implement auto-migration handling for compatible transforms.

## 🟩 Phase 2: Spatial Charter & Causal Propagation (COMPLETED)
- [x] Replace global tick (C=∞) with Causal Speed Constant (C=8).
- [x] Implement semantic lease manager over spatial hypergraph partitions (`SpatialCharter`).
- [x] Implement O(1) conflict check via spatial hash map.
- [x] Introduce the Watchguard Process to verify spatial lease claims.
- [x] Verify causal invariance holds for multiple simulated events across different locations.

## 🟩 Phase 3: Pawn Cognition & Emergent Economy (COMPLETED — GPU NN deferred)
- [x] Expand Layer 2 (Behaviour Graph) significantly (ThirstScorer/DrinkAction, FatigueScorer/RestAction, WaterSource, RestSpot).
- [x] Implement full "needs and capabilities" logic for all pawns (QuestBoard with requester/chunk/status, Capabilities component, need-posting on drive threshold).
- [x] Observe and verify emergent economy and trade cycles (no scripted quests); needs posted when drives cross threshold.
- [x] Implement item identity and actor history (ItemId, ItemIdentity, ItemHistory; record on eat/drink).
- [x] GPU NN upgrade: Scale to 64–256 drives via wgpu/Torch inference (deferred).

## 🟩 Phase 3.5: Minimal Bevy 2D Visualizer (COMPLETED)
A small task between Phase 3 completion and Phase 4. Goal: watch the simulation visually to debug and evaluate observer mode — "sit back and watch your colony and feel something."

- [x] Render the chunk grid.
- [x] Render pawns as colored circles, color mapped to dominant drive.
- [x] Render food/water sources as icons.
- [x] Render charter grant/deny as a brief flash on the contested chunk.
- [x] Render the quest board as a simple text overlay.
- [x] **Post-3.5**: Absorb big-brain into `lib/utility_ai` (Bevy 0.18); Position→Transform sync; demo pawn wander; `ActivityLog` + expanded UI (sim status, legend, quests, activity feed).

Not cosmetic: visual feedback reveals clustering, causal delay at C=8, and charter contention in ways logs cannot. Success = observer-mode proof is easier to evaluate by watching the screen than by reading the terminal.

## 🟩 Phase 3.75: Infrastructure Modernization (r2026t) (COMPLETED)
Standardizing the repo for AI agents and reproducible builds.

- [x] Pin toolchain to `stable` via `rust-toolchain.toml`.
- [x] Hoist external dependencies to `[workspace.dependencies]` in root `Cargo.toml`.
- [x] Centralize workspace-wide linting policy (warnings as errors).
- [x] Implement `external/` repository standard for third-party clones.
- [x] Audit all `TODO` statements and format with ISO dates.
- [x] Consolidate `baseline-agent-practices` and establish filesystem safety rules.
- [x] Add "Run Programs With Timeout" to baseline (reasonable timeouts for runs so they don't hang indefinitely).

## 🟩 Post–Phase 4.1: Headless & Scorer Fixes (COMPLETED)
Verification and stability so the sim actually runs in both headless and headed mode.

- [x] **Scorer entity vs actor**: BigBrain scorer entities only have `Actor(pawn)`; pawn state (NeuralNetworkComponent, Position, MovementTarget) lives on the actor. Updated all four scorers in `lib/ai/ai.rs` (hunger, thirst, fatigue, needs_to_move) to query `(&Actor, &mut Score)` and look up the actor's components. Without this, scores stayed 0 and pawns never ate/drank/rested.
- [x] **Score clamp**: Clamp scorer outputs to `[0.0, 1.0]` so `Score::set()` never panics when needs exceed 1.0 (e.g. after eating) in headed mode.
- [x] **Headless 10k run**: `cargo run -p simrard-bin -- --headless-test` reaches 10k ticks with all pawns alive; eat/drink/rest and resource consumption confirmed via report counters.

## 🟥 Phase 4 (Active, Ordered In ImplementationPlan)
Canonical checklist and execution order live in `ImplementationPlan.md`.

- [x] Run Phase 4 prerequisite gates (C constant confirmation, The System precondition graph task, DuckDB sync strategy decision, narrative-energy scope tie-in).
- [x] Implement Thinker stability improvements before quest acceptance (hysteresis + picker-default decision).
- [x] Implement Quest Acceptance & Economy lifecycle.
- [x] Add targeted observer UI verification slice (resource levels + quest states/providers).
- [x] Implement minimal SimLife pressure layer before recipe discovery.
- [x] Implement recipe discovery + teaching propagation.
- [ ] DuckDB staged rollout:
  - [x] D1 foundation now (ECS mirror + sync tests, no behavior coupling).
  - [x] D2 provider-ranking integration after economy baseline verification.
- [ ] Implement "Narrative Energy" / god power mechanics only if The System is in active scope for this phase.
- [ ] Implement Multiplayer capabilities (deferred; explicit entry criteria required).
- [ ] GPU NN upgrade: Scale to 64–256 drives via wgpu/Torch inference (deferred; explicit entry criteria required).
- [x] Goal-directed movement status: verified against code/tests and reclassified complete.

## ⚙️ Engine Tech Debt (Utility AI)
- [x] **Hysteresis**: Implemented in `Thinker` (Phase 4.3.1 in `ImplementationPlan.md`).
- [x] **Repeat-action cooldown**: Added configurable `repeat_action_cooldown_ticks` in `Thinker`; pawn brain set to `3` ticks (Phase 4.3.2 in `ImplementationPlan.md`).
- [x] **Picker Defaults**: Resolved as explicit "Picker is required" contract with assertive build-time panic message (Phase 4.3.4 in `ImplementationPlan.md`).

## 📝 Release Checklist
- [ ] **Toolchain Compatibility**: Verify `rust-toolchain.toml` is compatible with current Bevy version before bumping major versions.
