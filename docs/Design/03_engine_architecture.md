# **PART 3 — ENGINE & SIMULATION ARCHITECTURE**

## **3.1  Runtime Substrate — Bevy**

Bevy (Rust) is the chosen engine substrate. Key properties that make it compatible with this architecture:

* Engine internals are implemented using the same ECS and plugin system available to developers. True dogfooding — no privileged engine layer.
* The scheduler analyses system data access patterns at build time to determine safe parallel execution order. Causal independence is provable at compile time for declared systems.
* Plugin system is the primary extension mechanism. Spatial partitioning and the charter system are plugins, not engine modifications.
* Pre-1.0: pin versions, budget migration time between majors. AI-assisted rewrites make version migrations tractable.

**Current stack**: Bevy 0.18 across the workspace. The declarative utility AI (Layer 2 behaviour graph) is implemented in-house in `lib/utility_ai` (absorbed from big-brain, upgraded from Bevy 0.17 to 0.18); consumed by `lib/ai` and the game binary.

**Compute assignment by tier:**

| Tiers | Compute | Update rate |
| :---- | :---- | :---- |
| 1–3 (sapient, adaptive, reactive) | RTX 4080 GPU, tier-grouped SoA batches | 20–100Hz |
| 4 (reflex) | CPU preferred (branchy logic, large count) | 10Hz |
| 5–6 (plant/fungal) | CPU (Gray-Scott reaction-diffusion) | 4Hz |
| 7–8 (chemical/mineral) | CPU (conditional logic, CPU-friendly) | 1Hz |
| 9 (energy) | CPU (pre-computed map + time function) | near-zero cost |
| A6000 | Research/distillation only. Not a co-processor. | — |

## **3.2  Causal Substrate — Wolfram/Gorard Model**

The theoretical foundation is the Wolfram Physics Project's causal graph model, particularly Jonathan Gorard's formalisation of causal invariance. The key result: two operations are safe to run in parallel if and only if they produce no causal edge between them — i.e., neither operation's output is the other's input.

| Causal Invariance (operational definition) All valid orderings of causally independent operations produce identical results. Parallelism is safe exactly at the boundaries of causal independence. Those boundaries can be declared statically if data relationship topology is known. |
| :---- |

The practical consequence: there are no global ticks. There is only local causal propagation.

## **3.3  Causal Speed Constant (C)**

C is the maximum causal propagation rate — chunk-distances per causal step. It is a world constant tuned for gameplay feel, not a performance parameter.

| C \= 1:    maximum locality, maximum parallelism |
| :---- |
|           a raid announcement takes many steps to reach distant pawns |
|           information delay is a physical constraint |
|   |
| C \= 8-16: middle ground (recommended starting point) |
|           a fire takes a few causal steps to be 'known' at map far end |
|           creates natural drama: danger spreads before anyone reacts |
|   |
| C \= ∞:    global tick — RimWorld's model — avoid |

Two processes more than C×T causal steps apart at time T cannot have interacted. The charter prunes its conflict graph based on this — processes far enough apart are provably non-interacting without checking lease declarations.

The player UI pinging a unit ('run back to base') is modelled as 'very strong intuition something is wrong' — a narrative device that bends but does not break the causal model. Ansible devices (late-game unlock or rare resource) compress effective C for communication, enabling colony-wide knowledge synchronisation. Ansible communication routes through Tier 3 (Global Event Queue) only. Ansibles affect information propagation, not physical action coordination; charter leases remain spatial.

## **3.4  Three-Tier Coordination**

| Tier 1 — CRDT Zone (monotonic operations) |
| :---- |
|   All accumulation: damage, growth, hunger, resource counts |
|   Drive state (hunger, thirst, fatigue, etc.) is owned by a single writer (heartbeat); coordination-free by writer exclusivity, not by CRDT. True CRDTs apply to unbounded monotonic counters (e.g. damage totals, resource counts), not clamped drive values. |
|   No coordination required. Mathematically proven parallel-safe. |
|   Governed by CALM theorem: monotonic programs need no coordination. |
|   |
| Tier 2 — Spatial Charter (non-monotonic, local operations) |
|   Claims, reservations, deletions, 'exactly one pawn gets this' |
|   Semantic lease manager over spatial hypergraph partitions |
|   O(1) conflict check via spatial hash map |
|   |
| Tier 3 — Global Event Queue (non-local, rare operations) |
|   Raid announcements, trader arrivals, weather events |
|   Sequential, by design. Rare enough that cost is irrelevant. |

## **3.5  Spatial Charter Detail**

Each process (pawn job, building tick, environmental system) declares a spatial lease: primary chunk(s) \+ fringe margin. The charter maintains a spatial hash map: chunk → active leases.

| Entity: Pawn\_Marta |
| :---- |
| SpatialLease: { |
|   primary: chunk(14,8), |
|   fringe:  chunks(13-15, 7-9)   \# adjacent, could be affected this step |
| } |
| Intent: { |
|   read:  \[Food, Reservation\], |
|   write: \[Position, JobState\] |
| } |

Conflict check: does my requested chunk set intersect any active lease with incompatible intent? Hash lookup, O(1) per chunk. The expensive cases — 40 pawns converging on one room — are exactly the cases that genuinely have causal dependencies and should serialise.

## **3.6  Watchguard Process**

The watchguard is a postcondition verifier on spatial lease claims, not the primary safety mechanism. It walks behind edge processes and checks for fringe bleed — cases where actual data access exceeded declared lease boundary.

Over time the watchguard builds a statistical model per process type:

| Process: PawnHaulingJob |
| :---- |
| Declared lease: \+64 |
| Observed access pattern (10k executions): |
|   P(stays within \+64): 0.73 |
|   P(needs \+67):        0.19 |
|   P(needs \+72):        0.07 |
| Recommendation: default lease → \+72   (confidence: 0.99) |

The system self-tunes toward minimum sufficient leases. As a side effect: the watchguard's access pattern database is a continuous profiling instrument showing which systems are most causally entangled and where C budget is being saturated.

## **3.7  Data Layer Stack**

| ┌─────────────────────────────────────────────────────┐ |
| :---- |
| │  SCHEMA LAYER                                        │ |
| │  Versioned field definitions \+ migration transforms  │ |
| ├─────────────────────────────────────────────────────┤ |
| │  TRANSFORM LAYER                                     │ |
| │  Semantic diffs \+ contract declarations              │ |
| │  SAT solver validates before solidification          │ |
| ├─────────────────────────────────────────────────────┤ |
| │  SOLIDIFIED ARTIFACT (cache)                         │ |
| │  Hot-reloadable. Never canonical.                    │ |
| ├─────────────────────────────────────────────────────┤ |
| │  ESSENTIAL STATE  (Bevy ECS physical storage)        │ |
| │  Only mutable through chartered operations           │ |
| ├─────────────────────────────────────────────────────┤ |
| │  DERIVED STATE (dataflow graph)                      │ |
| │  Auto-invalidated \+ recomputed. Salsa-style.         │ |
| ├─────────────────────────────────────────────────────┤ |
| │  QUERY LAYER (exploratory — see Explore Notes)        │ |
| │  Relational cross-component queries                  │ |
| │  Vector index for similarity/semantic queries        │ |
| ├─────────────────────────────────────────────────────┤ |
| │  COORDINATION LAYER                                  │ |
| │  CRDT  |  Spatial Charter  |  Global Event Queue     │ |
| └─────────────────────────────────────────────────────┘ |
|          time: wall | sim | causal — always distinct |

Three explicit time types. Never conflate them. Sim time is wall time scaled by a player-controlled factor (SimTimeScale). It is distinct from causal sequence numbers, which are independent of wall time and controlled exclusively by the simulation. Causal time (charter sequence numbers) is ordered within a step, independent of wall time. Two events at the same wall-time instant may have defined causal ordering or be genuinely concurrent — the distinction is structural, not timing-based.

| 📝  EXPLORE — Engine Architecture |
| :---- |
| →  DuckDB integration with Bevy ECS — maintaining synchronised columnar views of ECS component tables for relational queries. What is the synchronisation cost? Is push (ECS writes trigger DuckDB update) or pull (DuckDB queries ECS directly via FFI) more appropriate? |
| →  Vector index for pawn-task matching — embedding pawn skill/trait profiles and task requirement profiles in the same latent space. 'Find the three pawns most suited to this task' becomes a vector distance query. Library selection (Qdrant embedded vs. custom). |
| →  C tuning as a gameplay setting — should players be able to adjust C? A 'slow information' world vs. a 'tight coordination' world. Accessibility implications. |
| →  Entity identity — Bevy's index+generation counter as item identity. Confirm this is sufficient for all duplicate-detection needs or whether additional identity layer is required for cross-save / multiplayer scenarios. |

---

## **3.8  1000Hz Scheduling Grid**

The master clock runs at 1000 ticks per second. This is not a global tick and does not conflict with the causal propagation model (§3.2). It is the **temporal resolution of the ECS step frequency** — a scheduling grid fine enough that any event can be slotted between any other event without needing fractional step identifiers.

**Reconciliation with the causal model:** Within each 1ms window, the causal engine processes all steps whose input causal edges are satisfied. The update rate table below is a throughput *guarantee* per system, not a synchronization barrier. Parallelism is still determined by causal independence (§3.2), not by tick alignment. C still governs how far causal effects propagate per step. The 1000Hz grid ensures there are always enough slots available; C determines which slots are causally connected.

**Causal chain example at 1000Hz resolution:**
```
tick 0:   food enters mouth
tick 3:   digestion reaction starts (Tier 7 chemistry)
tick 12:  glucose chemical rises (biochemistry update)
tick 18:  hunger drive drops (receptor fires, locus updated)
tick 19:  reward signal fires (drive-reducer detected)
tick 20:  Decision Layer weights update (LTW begins moving)
```
At 60Hz these all collapse into one tick. The causal chain has no room to breathe. Third effects cannot interpose. Emergence requires this breathing room.

**Natural update rates (all are integer divisors of 1000):**

| System | Rate | Every N ticks | Notes |
| :---- | :---- | :---- | :---- |
| Rendering | 60Hz | 17 | Visual frame |
| Tier 1 (sapient) neural tick | 100Hz | 10 | Responsive |
| Tier 2 (adaptive) neural tick | 50Hz | 20 | |
| Tier 3 (reactive) neural tick | 20Hz | 50 | |
| Tier 4 (reflex) neural tick | 10Hz | 100 | |
| Biochemistry (all tiers) | 20Hz | 50 | Drives, metabolism |
| Tier 5–6 (plant/fungal) growth | 4Hz | 250 | Gray-Scott update |
| Tier 7–8 (chemistry/mineral) | 1Hz | 1000 | Slow substrate changes |
| Dendritic migration (Tier 1/2) | 1Hz | 1000 | Background rewiring |
| Population/genetics events | 0.1Hz | 10,000 | Reproduction, mutation |

Nothing updates simultaneously. The 1000Hz grain makes this scheduling natural rather than engineered. Serial execution is a real constraint on physical hardware — the grid acknowledges this while maximizing causal fidelity within it.

---

## **3.9  Observability — Watching Without Watching Everything**

The observability model is the technical implementation of Observer mode gameplay (see `01_design_philosophy §1.3`). The goal is to detect interesting things without logging everything.

**Always on (near-zero cost):**
- Population counter per species. Increment on birth, decrement on death.
- Extinction flag: if counter hits 0, log extinction event with timestamp and region.
- Danger threshold flag: if counter drops below species-defined minimum viable population (~50 for genetic diversity, 2 for sexual reproduction), log warning.
- Chemical concentration histogram per region, sampled at 1Hz.

**Always on (small cost):**
- Circular buffer of significant deviations: any state change above a threshold magnitude (configurable per system). Chemical concentration drops >X% in one tick. Population crosses a round number downward. A pawn's drive hits maximum. This is the signal layer — most ticks most agents are boring.

**On-demand (triggered by threshold breach):**
- Full causal trace for a specific agent or species, reading back through the circular buffer to reconstruct why a deviation happened.
- `--benchtest` flag: profiles each agent tier on both CPU and GPU at various population sizes, finds the crossover point where GPU batching wins. Not for launch — for tuning Tier 3 CPU/GPU assignment.

**The practical case:** "Why is Vitamin B disappearing from the northern region?" is answerable without logging everything. Chemical histogram shows northern soil B-equivalent dropping over 200 ticks. Circular buffer shows fungal redistribution rate in that region dropped at tick 18,340. Population log shows decomposer insect count fell below viable threshold at tick 18,100. Something ate the decomposers. Trace back from there.
