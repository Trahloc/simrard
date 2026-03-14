# **PART 4 — AGENT COGNITION**

*This section distills the Grand (1997) biochemical neural architecture and extends it for Simrard's tier stack and GPU deployment target. The three-layer architecture (NN → Behaviour Graph → Charter) is a hard invariant established in `00_core_constraints`.*

---

## **4.1 Three-Layer Architecture**

Every pawn and animal has a three-layer cognitive architecture. The NN is not a black box — it maps to the Grand biochemical model where internal chemical states are observable, meaningful, and fully inspectable at runtime.

| Layer 1 — NN (felt experience) |
| :---- |
|   Inputs:  sensory data from spatial charter region |
|   Outputs: drive concentration vector (biochemical state — see §4.3) |
|   Model:   Grand biochemical NN (full specification in §4.4) |

| Layer 2 — Behaviour Graph (decision) |
| :---- |
|   Inputs:  drive concentrations from Layer 1 via dual-scorer interface (see §4.6) |
|   Outputs: concrete job selection |
|   Form:    declarative graph nodes (`lib/utility_ai`), not imperative code |
|            mods insert/remove/reorder nodes, never patch logic |

| Layer 3 — Charter (world interaction) |
| :---- |
|   Inputs:  chosen job |
|   Outputs: chartered spatial operations, CRDT accumulations |

Animals simplify or remove Layer 2. Tier 3 uses near-direct NN→charter with minimal scoring. Tier 4 has no Layer 2 — biochemical drive magnitudes map directly to near-fixed behaviors.

---

## **4.2 Agent Tier Specifications**

### Tier 1 — Sapient (Pawns / Norns)

The cognitively interesting agents. Social, linguistic, relationship-forming, capable of deliberate world-reshaping. Everything below this tier is background ecology.

**Population:** 64 normal operations, up to 256 during raids.

**Neural Architecture:** Extended C1/C3-inspired. Additional lobe types beyond the base Grand architecture:
- **Social lobe**: tracks relationships with specific named individuals. Emitter-receptor pairs keyed to agent identity signals.
- **Memory lobe**: episodic associations beyond Concept Space pattern-matching. Named individuals, locations, past interactions.
- **Extended drives**: existential and social axes not present in lower tiers (see §4.3).
- **Language lobes**: expanded noun/verb space. Subject-verb-object capable (vs. C1's verb-object only).

| Parameter | Value |
| :---- | :---- |
| Neurons | ~8,000–20,000 |
| Synapses | ~80,000–200,000 |
| Lobes | 16–24 |
| Concept Space | 5,000–15,000 cells |
| Drive axes | ~26 (§4.3) |
| Named biochemical states | ~52 |
| Biochemistry | Full + personality-influencing hormonal drift |
| Language | Full verb-object + subject extension |
| Dendritic migration | Active throughout life |
| Memory per agent FP16 | ~2–4MB |
| VRAM @ 256 agents × 3MB | ~768MB |

**Raider/Trader Note:** Raiders are Tier 1 architecture with specialist-trained LTW weights and low susceptibility to new reinforcement. They execute well but don't grow. A pawn is high susceptibility, active migration, LTW still being written. The raider is a sharpened knife. The pawn is a person.

*Promotion mechanic:* captured raider → join colony → swap to fully active Tier 1 mode, initialize weights from Tier 1 specialist baseline, enable migration and susceptibility. Architecturally natural; narratively interesting.

**Runs on:** GPU (RTX 4080), Tier 1 batch, 100Hz neural tick.

---

### Tier 2 — Adaptive (Mammals, Birds)

Meaningful individual behavior. Learns within lifetime. Forms associations. Feels like a real animal — consistent personality, recognizable responses to familiar situations.

**Population:** ~1,000 active.

Full C1 Norn fidelity at design-intent scale — 20,000 synapses, not the memory-compromised 5,000 of the shipped game.

| Parameter | Value |
| :---- | :---- |
| Neurons | ~1,000 |
| Synapses | ~20,000 (C1 design intent) |
| Lobes | 9 (full C1 brain model) |
| Concept Space | 640 cells |
| Drives | 6–8 (physiological + psychological + social subset; no existential) |
| Biochemistry | Full C1 including hormonal modulation |
| Language | Optional simple signals |
| Dendritic migration | Yes, background every ~50 ticks |
| Memory per agent FP16 | ~400–500KB |
| VRAM @ 1,000 agents × 450KB | ~450MB |

**Runs on:** GPU (RTX 4080), Tier 2 batch, 50Hz neural tick.

---

### Tier 3 — Reactive (Reptiles, Fish)

Drive-based behavior with shallow learning. Key architectural distinction from Tier 4: **individual recognition exists** — a lizard can distinguish *you specifically* from other humans. Implemented as learned Concept Space features, not a dedicated lobe.

**Population:** ~5,000 active.

| Parameter | Value |
| :---- | :---- |
| Neurons | ~400–600 |
| Synapses | ~3,000–5,000 |
| Lobes | 6–7 |
| Concept Space | 64–128 cells (slow migration) |
| Drives | 4–5 (hunger, fear, reproduction, territory, temperature) |
| Biochemistry | Simplified |
| Dendritic migration | Slow, infrequent |
| Memory per agent FP16 | ~50–80KB |
| VRAM @ 5,000 agents × 65KB | ~325MB |

**Runs on:** GPU (RTX 4080) or CPU depending on benchtest results (see §3.8). 20Hz neural tick.

---

### Tier 4 — Reflex (Insects, Simple Invertebrates)

Near-fixed wiring. **STW only — no LTW.** Short-term habituation exists (the fruit fly learns not to fly out of the jar) but nothing persists across a reset cycle. The lesson fades. Rich behavior comes from population density and ecological role, not individual cognition.

**Population:** ~10,000+ active.

| Parameter | Value |
| :---- | :---- |
| Neurons | ~100–300 |
| Synapses | ~500–1,500 |
| Lobes | 3–4 (stimulus, drives, attention, decision) |
| Concept Space | None |
| Drives | 2–3 (hunger, fear, reproduction) |
| Biochemistry | Minimal — metabolism only |
| Dendritic migration | None. Static wiring at birth. |
| STW learning | Yes — habituation only |
| LTW | None |
| Memory per agent FP16 | ~10–20KB |
| VRAM @ 10,000 agents × 15KB | ~150MB |

**Runs on:** CPU preferred (branchy logic, large count, cheap per-agent). 10Hz neural tick.

---

## **4.3 The 52-Drive Set**

Drives come in opposing poles — each pole is a distinct biochemical state with its own drive-raiser/drive-reducer signature, not a sign flip on a single value. This matters architecturally: "hungry for protein" and "full" interact differently with the reward system. The Tier column indicates which agent classes carry this drive axis.

**Phase 0 uses 8 bootstrap drives:** Hunger, Thirst, Fatigue, Curiosity, Social, Fear, Industriousness, Comfort. The full set below is Phase 3. No redesign required — Phase 3 expands the chemical space; the architecture is identical.

### Physiological (~18 named states)

| Drive Axis | In-Need Pole | Fine Pole | Tiers |
| :---- | :---- | :---- | :---- |
| Pain | hurt | well | 1–4 |
| Protein hunger | hungry | full | 1–4 |
| Fat hunger | hungry | full | 1–4 |
| Carb hunger | hungry | full | 1–4 |
| Thirst | parched | quenched | 1–4 |
| Temperature | cold | hot | 1–4 |
| Humidity | arid | clammy | 1–3 |
| Tiredness | tired | alert | 1–3 |
| Sleepiness | sleepy | awake | 1–3 |

*Thirst urgency curve is steeper than hunger — cognitive impairment emerges early from biochemistry, not a scripted debuff. Temperature and humidity compound: hot+arid and hot+clammy are biochemically distinct compound states without scripting.*

### Psychological — Valence/Arousal (~8 named states)

C3 split mood into two drives deliberately — a pawn can have both elevated simultaneously, producing states neither pole alone creates. Simrard keeps this split.

| Drive Axis | In-Need Pole | Fine Pole | Tiers |
| :---- | :---- | :---- | :---- |
| Mood valence | despair | hopeful | 1–2 |
| Mood arousal | numb | elated | 1–2 |
| Boredom | bored | excited | 1–2 |
| Anger | angry | peaceful | 1–3 |

*Crossing the axes: hopeful+elated = flourishing. hopeful+numb = quietly okay. despair+elated = manic. despair+numb = catatonic. All four states emerge from two chemical axes with no scripting.*

### Social (~8 named states)

| Drive Axis | In-Need Pole | Fine Pole | Tiers |
| :---- | :---- | :---- | :---- |
| Loneliness | lonely | content | 1–2 |
| Crowding | crowded | serene | 1–2 |
| Social friction | unfriendly | quiet | 1–2 |
| Comfort/home | homesick | secure | 1–2 |

### Existential — Tier 1 Only (~12 named states)

Not hardcoded personality traits. These emerge from hormonal drift interacting with life history. A pawn who repeatedly fails accumulates "incompetent" pressure. A pawn never given choices accumulates "controlled" pressure. Behavior follows from chemistry.

| Drive Axis | In-Need Pole | Fine Pole |
| :---- | :---- | :---- |
| Purpose | purposeless | fulfilled |
| Status | diminished | respected |
| Autonomy | controlled | free |
| Mastery | incompetent | skilled |
| Novelty-seeking | settled | restless |
| Safety-seeking | reckless | anxious |

### Functional — Navigation (~6 named states)

| Drive Axis | In-Need Pole | Fine Pole | Tiers |
| :---- | :---- | :---- | :---- |
| Go in | exposed | sheltered | 1–3 |
| Go out | confined | free-ranging | 1–3 |
| Wait | impatient | patient | 1–2 |

*Novelty-seeking high → go-out drive fires more easily. Safety-seeking high → go-in drive fires more easily. The interaction is emergent, not scripted.*

---

## **4.4 Neural Network Architecture**

*Grand/Cliff technical foundation adapted for GPU deployment.*

### Brain Model Overview

Each Tier 1–2 agent has a heterogeneous neural network subdivided into **lobes** — groups of neurons sharing identical parameters. Cells in each lobe connect to up to two source lobes. The architecture is biologically plausible and computable bottom-up with minimal top-down constructs.

### Neuron Parameters (per lobe — all neurons in lobe share these)

| Parameter | Description |
| :---- | :---- |
| Input types | 0, 1, or 2 dendrite classes; each pulls from a different source lobe |
| Input gain | Scalar multiplier on incoming signals |
| Rest state | Default internal state when unperturbed |
| Relaxation rate | Rate internal state exponentially returns to rest after perturbation |
| Threshold | Output = 0 if state ≤ threshold; output = state if state > threshold |
| SVRule | State-Variable Rule: genetically defined expression computing new internal state |

**SVRule examples:**
```
state PLUS type0                    → add type0 inputs to current state
state PLUS type0 MINUS type1        → type0 excitatory, type1 inhibitory
anded0                              → AND of type0 inputs; ignores previous state
state PLUS type0 TIMES chem2        → input modulated by chemoreceptor value
```

SVRules are interpreted opcodes designed to be fast, fail-safe, and mutation-proof. Any byte value produces a valid (possibly no-op) expression — genetic mutations can never cause crashes.

**Relaxation dynamics:** The further state drifts from rest, the faster it relaxes back — acts as both a damping mechanism and an input integrator. Neuron state reflects both intensity and frequency of stimuli.

### Synapse / Dendrite Parameters

| Parameter | Description |
| :---- | :---- |
| STW | Short-term weight — modulates incoming signal |
| LTW | Long-term weight — rest state for STW; slow-moving average |
| STW relaxation rate | Rate STW decays toward LTW |
| LTW relaxation rate | Rate LTW rises toward STW (slower than STW) |
| Susceptibility | Current sensitivity to reinforcement |
| Susceptibility relaxation rate | Half-life of susceptibility |
| Strength | Controls dendritic migration / disconnection |
| Reinforcement SVRule | Computes STW changes |
| Susceptibility SVRule | Computes sensitivity changes |
| Strength gain/loss SVRules | Compute growth and atrophy |

**STW/LTW dual timescale:** STW reacts strongly to individual reinforcement events. LTW is a slow moving average. Immediate negative experience → strong STW drop. Over time LTW moderates: "situation X isn't always as bad as first experience suggested." This is the core memory mechanism — fast response + slow statistical averaging. Do not collapse these into a single weight.

### Lobe Layout (C1 Baseline — Tier 2; Extended for Tier 1)

```
[STIMULUS LOBE]  [NOUNS LOBE]
       ↓               ↓
   [ATTENTION LOBE]  ←─── lateral inhibition → winner = current focus
       ↓
  [PERCEPTION LOBE] ← [VERBS] [MISC] [DRIVES]
       ↓
  [CONCEPT LOBE] — pattern matchers, 1–4 dendrites, fires on AND of inputs
       ↓
  [DECISION LOBE] — 16 cells (C1) / 64+ cells (Tier 1), one per possible action
       ↓
  [ACTION SCRIPTS]
```

Tier 1 additions: Social lobe, Memory lobe, extended Drives lobe, expanded language lobes.

### Reinforcement Learning

Drive-reduction reinforcement. Environmental stimuli produce DriveRaiser or DriveReducer chemicals:
```
DriveRaiser → Drive + Punishment
DriveReducer + Drive → Reward
```
Reward increases excitatory synapse weights. Punishment reinforces inhibitory ones. Context-dependent: reducing a non-present drive has no effect. Creatures learn to eat when hungry, not when full.

**Susceptibility gating:** A dendrite's susceptibility rises when it is conducting AND its target Decision cell is firing (this connection = current action in current context). Decays exponentially — allows deferred reward/punishment to reach recently-active synapses.

### GPU Deployment

**Memory layout: SoA, tier-grouped batches.**
Each tier processed as its own batch with tier-specific MAX_NEURONS ceiling. Do not pad Tier 4 agents to Tier 1 size.

```
tier1_state: [batch=256,   neurons=20000]
tier2_state: [batch=1000,  neurons=1000]
tier3_state: [batch=5000,  neurons=600]
```

**SVRules:** At gene expression time (birth, puberty), each SVRule compiles to a pre-built CUDA kernel via dispatch table. Finite grammar → finite table → no interpretation at runtime. Invalid byte sequences → no-op kernel.

**Dendritic migration:** Tier 1: async background every ~10 ticks. Tier 2: every ~50 ticks. Tier 3: slow/infrequent. Tier 4: none.

**Biochemistry:** CPU-side. Branchy conditional logic, CPU-friendly. Runs on 8500G cores in parallel with GPU forward pass. 4 cores: biochemistry + environment scripts. 4 cores: genome management + migration scheduling + game logic. iGPU: UI, HUD, debug visualization.

### Precision

| Data | Precision | Location |
| :---- | :---- | :---- |
| All neuron state, weights, relaxation | FP16 | GPU |
| Biochemistry concentrations | FP32 | CPU |
| Genome bytes | INT8 | CPU / NVMe |
| Hot cache (dormant agents) | FP16, zstd compressed | RAM |
| Population archive | INT8 quantized | NVMe |

---

## **4.5 Biochemistry System**

Four object types compose the system. This is the Grand biochemistry architecture applied per-creature. For world-scale application of the same architecture across simulation tiers, see `05_simulation_systems §5.8`.

**Chemicals:** Integer labels 0–N, each with a current FP32 concentration. No intrinsic properties — all behavior is gene-defined.

**Emitters (chemo-emitters):** Observe a locus byte in another system object; when it changes, adjust chemical output. The emitting code has no awareness of the emitter's existence — coupling is one-directional and data-driven.

| Field | Description |
| :---- | :---- |
| `source_system` | Organ/tissue identifier (brain, gut, muscle, etc.) |
| `source_subsystem` | Sub-tissue selector |
| `reads_field` | Locus byte being observed |
| `emits_chemical` | Integer label of chemical produced |
| `threshold` | Minimum locus value before emission begins |
| `rate` | Emission rate (exponential dynamics) |
| `gain` | Scalar multiplier on emission amount |
| `applicator` | Whether output adds to or sets the concentration |

**Reactions:** Form `iA + [jB] → [kC] + [lD]`. Concentration-dependent rate (exponential dynamics). Gene-defined, not physical law.

**Receptors (chemo-receptors):** Monitor a chemical concentration; write a locus byte. Attaching receptors to neuron parameters makes neurons chemically responsive — a receptor whose `write_field` points at a drive lobe neuron's rest-state parameter directly wires chemistry to felt experience.

| Field | Description |
| :---- | :---- |
| `target_system` | Organ/tissue identifier |
| `target_subsystem` | Sub-tissue selector |
| `write_field` | Locus byte to modify |
| `monitors_chemical` | Integer label of chemical tracked |
| `threshold` | Concentration below which receptor is inactive |
| `nominal` | Expected concentration at rest |
| `gain` | Scalar multiplier on output signal |
| `applicator` | Whether output replaces or modulates the locus |

The receptor/emitter interface is the universal cross-tier interface — see `01_design_philosophy §1.6` for why threshold conditionals must not appear at cross-tier boundaries. These fields are first-class transform targets in the mod architecture (see `02_mod_and_data_architecture §2.8`).

---

## **4.6 Layer 1→2 Interface: Dual Scorer Architecture**

The interface between NN drive outputs and the `lib/utility_ai` scorer graph. Drive concentrations from the Layer 1 NN flow into two classes of scorer:

**Biochemical scorers** read drive concentrations directly from the `NeuralNetworkComponent` ECS component:
```
score = drive_concentration × gene_defined_scale_factor
```
No world state. No context. Zero drive concentration = action score of zero — the "cannot learn to eat when not hungry" property is enforced structurally, not by convention. Gene-defined scale factors translate drive concentration to action affinity, allowing different species or individuals to have the same hunger level but different eating motivation.

**Contextual scorers** read ECS world state — entity proximity, item availability, social context, danger signals. They modulate the *timing and targeting* of an action, not the *motivation* for it. A contextual scorer can suppress an action when conditions are wrong (no food nearby, already occupied) but cannot generate motivation where the biochemical base is zero.

**Final action score:**
```
final_score = biochemical_base × contextual_modifier
```
Both layers are required. Biochemical base provides motivation; contextual modifier provides opportunity. This preserves the Grand property that behavior must be biochemically grounded while allowing it to be intelligently timed.

| Tier | Scorer complexity |
| :---- | :---- |
| Tier 1 | Full dual scorer. Gene-defined scale factors. LLM deliberation gate (see `05_simulation_systems §5.7`). |
| Tier 2 | Full dual scorer. No LLM layer. |
| Tier 3 | Reduced contextual scorers; near-direct biochemical→action. |
| Tier 4 | Biochemical scorers only; near-fixed gene-defined weights. No contextual modulation. |

---

## **4.7 Genetics**

### Genome Structure

Single haploid chromosome — string of bytes delimited by gene-marker punctuation. Gene header + body. Any byte (except markers) can mutate to any 8-bit value without crashing. Fail-safe by design.

Genome bytes are a transform target in the mod and data architecture. The field path `genome.lobe.N.svrule` is a versioned, namespaced entry in the same transform system as `needs.food.depletionRate` (see `02_mod_and_data_architecture §2.8`). The compiled SVRule kernel is the solidified artifact; the genome bytes are the canonical source. Hot-reload at pawn birth applies; a changed genome fires the solidification pipeline normally.

### Gene Header Fields

| Field | Purpose |
| :---- | :---- |
| Switch-on time | When during ontogeny this gene is expressed |
| Mutation flags | Whether this gene can be omitted, duplicated, mutated |
| Sex linkage | Instructions for both sexes; sex determines which are expressed |

### Reproduction

- Sexual: crossover at gene boundaries (not arbitrary bytes). Gene linkage proportional to separation distance.
- Crossover errors can produce gene omissions or duplications — a primary source of evolutionary novelty.
- Point mutations applied to gene bodies.
- Genome re-scanned at life-stage intervals — new genes switch on for development changes.

### What Genes Encode

**Structures, not functions.** Genes encode: brain lobe definitions, chemo-receptors, chemo-emitters, reactions, morphological details, senescence triggers. They do not directly encode fearlessness, hunger, or curiosity — these emerge from structural genes. Genotype and phenotype are separated by several abstraction layers.

Population and genetics events run at 0.1Hz (every 10,000 master-clock ticks) to allow simulation state to stabilize between generational intervals.
