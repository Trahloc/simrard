# **PART 5 — SIMULATION SYSTEMS**

## **5.1  Pawn Cognition — Steve Grand Lineage**

The full Grand biochemical neural architecture — lobe parameters, SVRules, synapse timing, the 52-drive set, tier specifications, dual-scorer Layer 1→2 interface, and genetics — is specified in `04_agent_cognition`. This section covers how the three-layer architecture connects to the simulation systems that surround it.

| Layer 1 — NN (felt experience) |
| :---- |
|   Inputs:  sensory data from spatial charter region |
|   Outputs: drive concentration vector |
|   Model:   Grand biochemical NN (see `04_agent_cognition §4.4`) |

| Layer 2 — Behaviour Graph (decision) |
| :---- |
|   Inputs:  drive concentrations via dual-scorer interface (`04_agent_cognition §4.6`) |
|   Outputs: concrete job selection |
|   Form:    declarative graph nodes (`lib/utility_ai`), not imperative code |
|            mods insert/remove/reorder nodes, never patch logic |

| Layer 3 — Charter (world interaction) |
| :---- |
|   Inputs:  chosen job |
|   Outputs: chartered spatial operations, CRDT accumulations |

Animals simplify or remove Layer 2. Tier 3: near-direct NN→charter. Tier 4: biochemical drives→near-fixed behaviors, no scorer graph.

## **5.2  The Emergent Economy**

The quest board is a needs-based emergent market, not a scripted quest system — and it is also the surface interface for The System (see §5.10). Pawns have needs (required inputs) and capabilities (producible outputs). The board is where unmet needs become visible. No designer scripted 'miners should mine when smiths need metal' — it falls out of agents posting and fulfilling needs.

When The System crystallizes a new technological insight for a civilization, a new need or capability type becomes legible to Tier 1 pawns. The board doesn't change — but new categories of needs and offers can now appear on it. The tech tree is expressed through what pawns can post, not through unlocked UI screens.

| Smith:   'I need iron'          → posts to board |
| :---- |
| Smelter: 'I can fulfill that'   → posts: 'I need ore' |
| Miner:   'I can fulfill that'   → accepts, begins mining |

Pawn selection is drive-weighted, not random. High industriousness \+ mining skill \= strong pull toward fulfilling ore requests. Lazy pawn ignores it until hunger makes the economic reward necessary. The economy is the gameplay at the Architect layer.

## **5.3  Currency & Contracts — Emergent, Not Scripted**

The game provides the substrate for exchange — items have identity, actors have needs, time is ordered causally. It does not dictate what currency looks like. The intent is that monetary systems emerge from agent behaviour rather than being imposed by designers.

Possible emergent forms (not scripted, observed):

* Commodity money — the most useful tradeable good becomes de facto currency
* Favours and promises — 'I'll get you ore if you give me 10% as refined product'
* Debt instruments — promises of future goods
* Reputation tokens — social credit based on promise-keeping track record

The NN biochemistry layer provides trust/suspicion drives that make promise-keeping and defection emotionally costly or rewarding to individual pawns. Contract enforcement is social, not mechanical.

## **5.4  Recipe Discovery — Procedural Within A Tree**

Knowledge is not unlocked. It is discovered, understood, and socially propagated at causal speed C.

| Creatures layer (hidden chemistry): |
| :---- |
|   Pawn exposed to substances → internal chemical state modifies |
|   Novel reaction detected → curiosity drive fires |
|   |
| Minecraft layer (surface experiment): |
|   Curiosity drive → experiment behaviour triggered |
|   Pawn combines items deliberately |
|   Combination crosses discovery threshold → recipe enters pawn knowledge graph |
|   |
| Propagation: |
|   Discovering pawn: knows immediately |
|   Adjacent pawns: learn at causal speed C via social contact |
|   Far colony: may not know for many causal steps |
|   Ansible device: compresses propagation to near-instant (late game) |

The recipe graph has topology (iron tools require smelting require ore require mining) — you cannot skip layers. The procedural element is which pawn discovers it, when, and how they describe it to others. Knowledge diversity within a colony is a resource.

## **5.5  Discovery As Research — SimX Layer**

Each Maxis sub-simulation is a physical place inside the world, not a UI screen. A pawn with geology skill enters a cave and finds the SimEarth layer. A microscope reaches the SimLife layer. These layers answer the 'why' questions the surface colony cannot.

* SimEarth explains why this biome has these minerals, why the water table shifted
* SimLife explains why the wolf population spiked, what they're eating, where they're going
* SimAnt reveals the ant colony's decision to expand was caused by their food stores depleting

Drama comes from below, not from scripted events. The surface colony responds to pressures it may not yet understand — because the pawns haven't discovered that layer yet.

## **5.6  Pawn Cognition: Limbic & Prefrontal Layers**

Pawn cognition is split into two named layers that map to the existing three-layer architecture:

* **The Limbic System** comprises Layers 1 and 2 — the drive vector (`NeuralNetworkComponent`, see `04_agent_cognition §4.4`) and the utility-AI behaviour graph (scorers + actions in `lib/utility_ai`). It runs every heartbeat, is always active, requires no LLM, and handles all autonomous survival behaviour. This is what keeps a pawn alive when nobody is watching.
* **The Prefrontal Layer** is an LLM (target: Qwen3-0.8B or equivalent small model) that activates only on social interaction triggers, significant emotional events, and direct player queries. It is not responsible for action selection. It generates speech acts which are parsed for valence and drive signals and fed back into the drive vector before the utility-AI layer evaluates. The words a pawn speaks are a cognitive step, not a narration of a decision already made.

Each pawn has a LoRA adapter (~4MB) representing their personality, loaded on top of the shared base model. Drive state is injected as structured context at inference time.

## **5.7  Speech Acts & Deliberation Gate**

LLM output is parsed into a structured `SpeechAct` containing: raw text content, valence (-1.0 hostile to +1.0 warm), optional target entity, and drive signals (a list of drive type + float delta pairs).

Drive signals from speech acts do not override the biochemical state. They are scaled by a **deliberation capacity** derived from the pawn's current drives — no new chemistry required:

```
urgency_floor    = normalize(max(pain, thirst_urgency, fear, acute_hunger))
mood_factor      = 0.5 + 0.5 × normalize(mood_arousal_drive)
deliberation_cap = (1.0 - urgency_floor) × mood_factor
llm_delta        = speech_act_delta × deliberation_cap × ±0.1_ceiling
```

A pawn in agony (urgency_floor near 1.0) cannot be talked down — the LLM contribution approaches zero. A calm, alert pawn (low urgency, high mood arousal) is maximally open to influence — up to the ±0.1/inference ceiling. The four valence/arousal quadrant states from `04_agent_cognition §4.3` map directly onto deliberation capacity:

| State | deliberation_cap | Meaning |
| :---- | :---- | :---- |
| hopeful + elated | high | Fully deliberative; words land |
| hopeful + numb | medium | Receptive but not expressive |
| despair + elated | low-medium | Manic; partially reachable |
| despair + numb | near-zero | Catatonic; speech barely registers |

This is not a designed behavior — it falls out of the mood valence/arousal axes already defined in `04_agent_cognition §4.3`.

The LLM fires only on: two pawns in social proximity with elevated social drives, a pawn experiencing a significant event (discovery, loss, fear), or a direct player query. It never fires every heartbeat. It runs async and non-blocking — the simulation never waits for LLM inference. Urgency gating means the LLM fires far less often under colony stress.

## **5.8  Conversation As Mechanic**

Because speech acts update drives, conversation is a first-class game mechanic. A player addressing a discouraged pawn with positive language can genuinely improve that pawn's comfort and industriousness drives — not through a cheat code but because the player's words are evaluated as a speech act with positive valence. Morale is a real system influenceable through communication.

Emotional state propagates socially via language. A pawn who witnessed something traumatic and speaks about it with another pawn may resolve their fear drive faster — or may propagate fear to the listener. This is not scripted. It emerges from the speech act and drive feedback architecture.


| 📝  EXPLORE — Simulation Systems |
| :---- |
| →  Knowledge graph structure — pawn knowledge is a graph, not a list. Does pawn A knowing recipe X imply they know its prerequisites? How does misremembering or partial knowledge work? |
| →  Promise/contract enforcement mechanics — what happens when a pawn defects on a promise? Social drives (shame, anger, reputation damage) are the enforcement mechanism. How does this interact with the NN layer? |
| →  SimX layer depth — are the sub-simulations full real-time simulations or are they computed lazily (simulated backward from observed surface effects)? The latter is far cheaper and may be more interesting. |
| →  Ansible as technology, not given — the path from C-speed information propagation to ansible-level coordination should itself be a discovery chain. What are the intermediate steps? |
| →  LLM sidecar architecture — Bevy communicates with the LLM via a local async socket. Game engine fires inference request and handles response non-blocking when it arrives. Failure mode: no speech output, simulation continues correctly without it. |
| →  Structured output format — Qwen function-calling mode should output speech text AND emotional annotation (valence + drive signals) in a single inference pass to avoid a second parsing call. |
| →  LoRA training pipeline — when and how are pawn LoRAs generated or evolved? Are they authored, procedurally generated at pawn creation, or do they drift over time based on the pawn's history? |
| →  Player speech evaluation — player words to a pawn evaluated with the same speech act parser. Implication: player can influence pawn emotional state through conversation quality, not just game commands. |
| →  LLM invocation rate under urgency gating — deliberation_cap near zero means the LLM rarely fires for stressed pawns. Measure actual inference rate across colony states to validate the sidecar architecture scales. |

---

## **5.9  Tiers 5–9 Simulation Specifications**

### Tier 5 — Vegetable (Plants)

Growth, energy transduction, uptake from soil chemistry. Not a neural network. **Reaction-diffusion (Gray-Scott model)** — each cell looks at immediate neighbors and applies a local chemical reaction rule. O(N) with no global solve. Growth, spread, branching, and pruning emerge from the local rules. The "organism" is a pattern in the field, not a discrete agent.

Plant uptake of soil chemistry is a **receptor gain on the soil concentration field** — the same emitter/receptor architecture used everywhere else. High concentration → receptor fires strongly → uptake fast. Low → uptake slow. Zero → nothing happens. Do not write `if concentration > threshold then plant.uptake()`. That is a scripted famine with extra steps. See `01_design_philosophy §1.6`.

**Runs on:** CPU. 4Hz (every 250 master-clock ticks). Cost: <1% of one core.

### Tier 6 — Fungal (Mycelium / Decomposition Network)

The redistribution layer. When something dies, its chemicals don't disappear — they enter the decomposition cycle, the fungal network absorbs and routes them, and returns them to soil chemistry. Without this layer, nutrients sink and don't cycle. Local depletion cascades emerge without any scripted famine event.

Also a Gray-Scott reaction-diffusion system but with different parameters — slower-growing, longer-lived, directional (grows toward nutrient sources). The interaction between Tier 5 and Tier 6 fields produces the living surface texture of the world.

**Runs on:** CPU. 4Hz update. Separate Gray-Scott field from Tier 5, independently tunable.

### Tier 7 — Chemical (Reaction Substrate)

The trophic chemistry layer. Defines which chemicals exist, how they transform, what produces what. This is the Grand biochemistry system applied at world scale rather than per-creature scale — the same four object types (chemicals, emitters, reactions, receptors) from `04_agent_cognition §4.5`, but wired to world processes instead of creature organs.

Emitters attach to processes (plant growth, decomposition, animal metabolism) and emit chemicals as byproducts. Receptors monitor concentrations and trigger downstream effects. The trophic chain is chemistry: Tier 5 organisms fix Tier 8 minerals + Tier 9 energy into base organics; Tier 4 insects concentrate those into compounds Tier 3 animals can use; Tier 3 concentrates further; Tier 1 sapients eat at the top. A pawn's nutrient deficiency is traceable down to soil chemistry via unbroken receptor/emitter chains.

**Runs on:** CPU. 1Hz update. Branchy conditional logic, CPU-friendly.

### Tier 8 — Mineral / Elemental (Inorganic Substrate)

Stable inorganic structures and elemental matter in all phases. Soil composition, stone types, mineral deposits, atmospheric gas composition, liquid water bodies. Changes extremely slowly — geological timescale in game time.

**Phase matters; atomic structure does not.** The primitive is the molecule, not the atom. Iron ore is iron ore, not Fe₂O₃ with tracked electron valence. CO₂ is a tracked atmospheric component, not two oxygen atoms bonded to carbon. Molecules yes, atomic physics no.

**Granularity is legibility-gated.** Iron exists in the substrate always. It is not accessible as a distinct mechanic until The System (§5.10) unlocks it for a civilization that has reached the relevant cognitive threshold. Before: a pawn sees "rock." After: they see "iron ore." The world didn't change — their legibility of it did.

**Atmospheric gases are Tier 8.** O₂%, CO₂%, nitrogen, methane, water vapor — tracked as regional bulk concentrations. Vacuum is a valid Tier 8 state. Never hardcode "atmosphere always present."

**Runs on:** CPU. Very low frequency update. Effectively a static map with slow diffusion and rare event-driven changes.

### Tier 9 — Energy (Abiotic Flux)

Light, heat, radiation. The boundary condition that drives everything. Tier 9 is an active thermal sink with a 2.7 K baseline: waste heat from upper-tier activity drains toward the sink each update under Newton cooling ($k \cdot (T_{local}-2.7)$). Day/night cycle, seasons, weather, and geothermal variation still shape available flux, but thermal dissipation is simulated state rather than a static lookup. Without continuous energy input, the system runs to equilibrium: everything dies, nothing moves.

**Runs on:** CPU. Active thermal boundary simulation (sink baseline + per-update cooling), with cumulative dissipation tracked for substrate stability monitoring.

---

## **5.10  The System — Technology Crystallization**

The one explicitly non-emergent element in an otherwise fully emergent design — and deliberately so.

Tiers 2–10 are inside the world, subject to its rules. A wolf doesn't discover metallurgy. Tier 1 is different: sapients model the world abstractly and deliberately reshape it based on that model. Technology is not a new capability appearing from nowhere — it is the world's underlying structure becoming **legible** to a mind sophisticated enough to read it. Iron was always in Tier 8. Smelting is the moment a civilization has accumulated enough relevant observations that the concept crystallizes.

**The System is the crystallization mechanism.** When a Tier 1 civilization meets the preconditions for a technological insight, The System bestows it — the "I know kung fu" moment. This is architecturally a **permeability membrane** between Tier 1 cognition and the lower tiers. Low-tech pawns interact with Tier 8 at coarse granularity (rock). High-tech pawns interact with finer granularity (iron ore, carbon content, alloy ratios) because The System has surfaced what was always there.

The pawns aren't smart enough to genuinely derive metallurgy from first principles — the simulation isn't that deep. The System is the acknowledged sanded edge. The design crib is LitRPG's solution to the same problem: the world reveals itself to those who have earned the legibility. The precondition model is world-state + agent-history, not a timer.

**The tech tree precondition design is future work.** The full precondition graph — what leads to what, at what threshold, with what branching — is a dedicated design sprint. See `06_next_steps §6.3` for the research task.
