---
name: Design doc corrections post-review
overview: "Apply Sonnet's triage: fix critical and significant items in the design docs and TODO.md so the docs match implementation reality and close the identified architectural gaps before Phase 4 or more engineers touch the codebase."
todos: []
isProject: false
---

# Design Doc Corrections (Post–Opus Review Triage)

Sonnet's triage is adopted. This plan implements the **Actually Critical** and **Significant** doc changes only. Moderate/minor items are noted at the end for backlog.

---

## 1. CRDT vs single-writer for drives (Critical)

**File:** [docs/Design/03_engine_architecture.md](docs/Design/03_engine_architecture.md)

**Location:** Section 3.4 "Three-Tier Coordination", Tier 1 — CRDT Zone.

**Change:** Clarify that Tier 1's "coordination-free" guarantee for drives comes from **single-writer exclusivity** (heartbeat owns drive writes), not from CRDT properties. Bounded floats are not CRDT-safe; the architecture avoids concurrent writers instead.

**Edit:** In the Tier 1 bullet list, replace or augment the line about "All accumulation: damage, growth, hunger, resource counts" so it states explicitly that drive state (hunger, thirst, fatigue, etc.) is owned by a single system (heartbeat) and is coordination-free by writer exclusivity; true CRDTs apply to unbounded monotonic counters (e.g. damage totals, resource counts), not clamped drive values.

---

## 2. Intent-follows-words: blending and caps (Critical)

**File:** [docs/Design/04_simulation_systems.md](docs/Design/04_simulation_systems.md)

**Location:** Section 4.7 "Speech Acts & Drive Feedback", paragraph that states "the words win" and "Drives update to reflect what was said".

**Change:** Add a design decision: speech-act drive deltas are **capped at ±0.1 per inference** and **blended** (e.g. 30% speech / 70% existing state). Words influence; they do not dictate. This avoids LLM output overriding the biochemical simulation and is more neurologically plausible.

**Edit:** Insert a short subsection or paragraph after the intent-follows-words rule stating the cap and blend formula, and that the rule applies to the *blended* result, not a hard override.

---

## 3. Ansible → Tier 3 only (Critical)

**File:** [docs/Design/03_engine_architecture.md](docs/Design/03_engine_architecture.md)

**Location:** End of section 3.3 "Causal Speed Constant (C)", where ansible devices are introduced.

**Change:** Add one explicit architectural decision: **Ansible communication routes through Tier 3 (Global Event Queue) only.** Ansibles affect information propagation, not physical action coordination. Charter leases remain spatial; charter pruning is unchanged.

**Edit:** One or two sentences immediately after the current ansible sentence.

---

## 4. Phase 0 success metric vs reality (Significant)

**File:** [TODO.md](TODO.md)

**Location:** Phase 0 section header / first bullet block.

**Change:** Add a note that Phase 0 **infrastructure** is COMPLETED; the **full success metric** (curiosity → fire discovery → teaching → zone formation) is deferred to Phase 4. Do not lower the metric in the design doc; only clarify status in TODO.

**Edit:** A single line or short paragraph under Phase 0 in TODO.md.

---

## 5. DuckDB and vector index → Explore (Significant)

**File:** [docs/Design/03_engine_architecture.md](docs/Design/03_engine_architecture.md)

**Location:** Section 3.7 "Data Layer Stack" — the ASCII diagram and any definitive mention of DuckDB/vector index as a committed tier.

**Change:** Remove DuckDB and vector index from the definitive stack diagram (or relabel that row as "Query layer (TBD / exploratory)"). Ensure the EXPLORE box already contains the DuckDB/vector questions; if not, add a bullet that the query layer implementation (DuckDB in-process, vector index) is a research question, not a commitment.

**Explicit diagram edit:** In the ASCII stack diagram in 3.7, replace the line reading `│  QUERY LAYER (DuckDB in-process + vector index)      │` with `│  QUERY LAYER (exploratory — see Explore Notes)        │`. Do not leave the diagram unchanged when editing the surrounding prose.

---

## 6. "No privileged access" enforcement (Significant)

**File:** [docs/Design/02_mod_and_data_architecture.md](docs/Design/02_mod_and_data_architecture.md)

**Location:** Opening paragraph: "hard invariant enforced by the build system".

**Change:** Replace with **enforced by convention and architecture** (not "by the build system"). Principle unchanged; avoid implying tooling that doesn't exist.

---

## 7. Sim time definition (Significant)

**File:** [docs/Design/03_engine_architecture.md](docs/Design/03_engine_architecture.md)

**Location:** Section 3.7, near "time: wall | sim | causal — always distinct".

**Change:** Add one sentence: **Sim time is wall time scaled by a player-controlled factor (SimTimeScale). It is distinct from causal sequence numbers, which are independent of wall time and controlled exclusively by the simulation.**

---

## Backlog (no doc edits in this pass)

- **Epoch assignment:** Document in a later pass: single-process, single incrementing u64, owned by transform pipeline; multiplayer epoch coordination = Phase 6+.
- **Watchguard stability:** Add to Explore in 03: "Convergence and oscillation prevention are open research questions."
- **LoRA memory / namespace forever:** Leave as-is or add to Explore Notes only when touching those sections.

---

## Execution order

1. 03_engine_architecture.md: CRDT/single-writer (3.4), Ansible Tier 3 (3.3), DuckDB/query layer (3.7), sim time (3.7).
2. 04_simulation_systems.md: speech-act blending and caps (4.7).
3. 02_mod_and_data_architecture.md: privileged access wording (opening).
4. TODO.md: Phase 0 note.

All edits are additive or minimal replacements; no structural reorganisation.
