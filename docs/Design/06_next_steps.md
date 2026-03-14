# **PART 6 — OPEN QUESTIONS & NEXT STEPS**

## **6.1  Foundational Questions (Answer Before Building)**

* What is C? The causal propagation constant needs a number before the simulation can be prototyped. Recommend: start at C=8, tune from Observer-mode playtesting.
* What is the player's default mode? Observer is implied as the base state — but does the game start with a colony already running, or does the player architect from the beginning?
* What is the minimum viable sub-simulation? Which Maxis layer is implemented first to make the surface simulation non-trivially interesting? SimLife (ecology) is the candidate — it drives the most surface-layer pressure with the least implementation complexity.
* Narrative energy / god power — commit or cut. If kept, it needs a name and a generation formula before any economy systems are designed around it.

## **6.2  Prototype Priorities (In Order)**

* Observer mode viability — can the 52-drive biochemical pawn system (Phase 3; 8-drive bootstrap for Phase 0) generate interesting drama with zero player input? This is the critical assumption. If it fails, the philosophy fails.
* Charter correctness — does the spatial lease system produce correct results under the kinds of concurrent access patterns a 50-pawn colony generates?
* Transform pipeline — schema declaration → SAT validation → solidification → hot reload. Must work end-to-end before any content is authored. Includes receptor/emitter schema spec (see §6.4).
* Recipe discovery loop — can a pawn discover a recipe without being told to? Does knowledge propagation feel natural?

## **6.3  Research Tasks (Named, Kickoff When Ready)**

**Tech Tree Precondition Graph — The System**
Conduct a systematic survey of RimWorld tech tree mods (primitive extension mods, advanced tech mods, tree-organization mods) before designing Simrard's precondition graph. Goal: identify design patterns that feel discovered vs. granted, and tree structures that support interesting branching without becoming mandatory optimization paths.
Deliverable: A precondition graph draft covering at least the neolithic-to-early-industrial range, with precondition triggers specified in terms of world-state + agent-history.

**Layer 1→2 Scale Factor Initialization**
The gene-defined `scale_factor` values that translate drive concentration to action affinity need starter values for playtesting. These determine species-level behavioral personalities (a wolf has different hunger→hunt scale factors than a deer). Requires: picking representative starting species, deciding scale factor ranges, running Phase 3 playtests.

**LLM Invocation Rate Benchmarking**
The deliberation gate means the LLM fires far less often under stressed colony conditions. Measure actual inference rate across representative colony states (normal play, raid, famine, cultural event). Validate that the async sidecar architecture handles peak load at normal-play invocation rates without queuing buildup.

---

## **6.4  Near-Term Pre-Content Tasks**

These must be completed before biochemistry content is authored via the mod system:

* **Receptor/emitter transform schema** — the YAML examples in `02_mod_and_data_architecture §2.8` need to be validated end-to-end through the solidification pipeline with at least one test receptor and one test emitter.
* **Genome as transform target** — confirm that `genome.lobe.N.svrule` round-trips through the transform → SAT validation → solidification → compiled kernel path.
* **Drive state persistence across saves** — confirm FP32 biochemistry concentrations serialize and restore correctly. Verify that restoring a mid-simulation save produces biochemically correct behavior within a few ticks.

---

## **6.5  Deferred (Do Not Design Yet)**

* Rendering pipeline — Bevy handles it. Customise after simulation is proven.
* Multiplayer — same architecture extends there but it is a separate problem with separate tradeoffs.
* UI/UX design — the diegetic mode transitions are defined. Specific UI implementation deferred until simulation layer is stable.
* Content — biomes, species, items, recipes. All downstream of architecture decisions.
* Monetisation — not relevant at this stage.
