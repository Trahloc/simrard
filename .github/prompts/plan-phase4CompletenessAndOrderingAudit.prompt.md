## Plan: Phase 4 Completeness And Ordering Audit

Roadmap is directionally strong but not complete as an execution artifact yet: there are contradictory status markers between planning docs, a few architecture-level prerequisites not represented as tasks, and one ordering issue that increases behavior instability risk. Recommended approach is to normalize the source of truth, then execute a dependency-first sequence: decision stability, economy loop, UI observability, environmental pressure, knowledge propagation, and only then advanced/deferred systems.

**Steps**
1. Normalize roadmap truth across planning docs. Mark completed items once in a canonical tracker and remove contradictory duplicates. Depends on no prior step.
2. Add a short prereq-gates section before remaining Phase 4 tasks. Include explicit gates: C constant confirmation, The System precondition-graph draft, and commit/cut decision for narrative energy formula. Depends on step 1.
3. Move Thinker stability work before quest economy expansion. Execute hysteresis and picker-default decision before quest acceptance to reduce oscillation risk in provider selection. Depends on step 2.
4. Execute quest acceptance and completion lifecycle after Thinker stability. Ensure one-provider-per-quest, lifecycle states visible, and quest closure semantics are explicit. Depends on step 3.
5. Move targeted UI/observability earlier. Implement only the UI slices that verify economy correctness (quest states, provider visibility, resource levels), then treat remaining visual polish as optional. Depends on step 4.
6. Run SimLife minimal layer before recipe discovery. Wire one concrete influence from SimLife into food/water pressure so discovery is tested under environmental dynamics, not static spawners. Depends on step 5.
7. Execute recipe discovery and teaching after SimLife. Keep scope minimal: one recipe, one propagation event kind, and visible logs/UI proof of spread. Depends on step 6.
8. Split DuckDB into two milestones so debt does not compound:
   8a. DuckDB foundation now: schema mirror/resource contract and sync direction decision (push or pull) with tests and no gameplay coupling.
   8b. Vector matching integration later: hook quest provider selection to vector queries after baseline economy behavior is stable and benchmarked.
   Step 8a depends on step 2 and can run in parallel with steps 3-5 if API surface is isolated. Step 8b depends on steps 4 and 8a.
9. Keep narrative energy aligned with The System milestone. If The System is active scope, include narrative-energy formula and spend semantics now as design tasks; if The System is deferred, defer narrative energy with it. Depends on step 2.
10. Reclassify deferred work clearly: GPU NN/LLM depth and multiplayer remain deferred, but each needs explicit entry criteria so they do not re-enter roadmap ambiguously. Depends on step 1.
11. Add verification gates per phase boundary:
   - Build/test gate
   - Headless long-run survival and economy progression gate
   - Observer-mode visual gate
   - Performance regression gate when DuckDB integration begins
   These gates apply after each major step and block promotion to the next phase.

**Relevant files**
- /home/trahloc/code/simrard/ImplementationPlan.md — Canonical phase checklist; needs ordering update, duplicate cleanup, and DuckDB split into foundation vs integration milestones.
- /home/trahloc/code/simrard/TODO.md — High-level roadmap; currently contains status contradictions (movement, hysteresis duplication) and must align to ImplementationPlan.
- /home/trahloc/code/simrard/docs/Design/00_core_constraints.md — States deferred boundaries; currently indicates DuckDB and narrative energy were deferred until foundation proof.
- /home/trahloc/code/simrard/docs/Design/03_engine_architecture.md — DuckDB and vector index architecture questions (sync strategy, index choice) to convert into explicit tasks.
- /home/trahloc/code/simrard/docs/Design/05_simulation_systems.md — The System integration with quest surface and legibility gating; informs ordering of SimLife/discovery/economy.
- /home/trahloc/code/simrard/docs/Design/06_next_steps.md — Explicit callout that narrative energy must be commit-or-cut before economy designs that depend on it.

**Verification**
1. Documentation consistency check: no task appears as both complete and deferred across plan files.
2. Dependency check: each phase item references prerequisites and successor validation criteria.
3. Sequence smoke test: after Thinker changes, verify reduced action thrash before enabling quest acceptance.
4. Economy proof: at least one open quest moves to in-progress then completed with one unique provider.
5. SimLife coupling proof: changing SimLife state changes at least one resource-pressure signal.
6. Discovery proof: one pawn discovers recipe and at least one pawn learns it through propagation path.
7. DuckDB foundation proof: ECS-to-DuckDB sync contract passes tests without altering quest behavior yet.

**Decisions**
- SimLife should precede recipe discovery (user preference captured).
- DuckDB should start now, but as bounded foundation first to avoid future rewrite debt while avoiding premature behavior coupling.
- Narrative energy should remain in scope only if The System milestone remains in scope; otherwise defer both together.
- Recommended ordering revision:
  1) 4.3 Thinker improvements
  2) 4.2 Quest acceptance and economy
  3) 4.5 Targeted observability UI
  4) 4.6 SimLife minimal layer
  5) 4.4 Discovery and knowledge propagation
  6) 4.7 and 4.8 deferred by explicit entry criteria

**Further Considerations**
1. DuckDB integration boundary recommendation:
Option A: Read-only advisory ranking for provider selection first (safer).
Option B: Hard replacement of heuristic provider selection (riskier).
Recommendation: Option A for one phase, then A/B compare outcomes.
2. Narrative energy design depth recommendation:
Option A: Minimal formula plus two spend actions for Phase 4.
Option B: Full economy coupling now.
Recommendation: Option A to validate feel before deep coupling.
3. Source-of-truth recommendation:
Option A: Keep both TODO and ImplementationPlan synchronized manually.
Option B: Make ImplementationPlan canonical and reduce TODO to milestone pointers.
Recommendation: Option B to avoid drift.
