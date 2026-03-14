# **CORE CONSTRAINTS AND DIRECTIVES**

**Additional engineering constraints (these are hard invariants — never violate them):**

- "Being right is soft; being obviously wrong is hard no." Never hard-code job logic into systems, never use a black-box AI that cannot be expressed as a transform, never ship a sim that dies without player input in Observer mode, never bake a design that blocks future causal propagation or GPU NN upgrade.
- Everything (core mechanics, pawn cognition, economy) must be expressed as transforms from day 1.
- Three-layer pawn cognition (NN → Behaviour Graph → Charter) is mandatory from the start.

- Start with a merged bootstrap skeleton using strengths from current open-source Bevy colony projects. Use TheBevyFlock/bevy_new_2d as the base template for clean ECS + hot-reload support. Add big-brain (or active Galaxy Brain fork) for the declarative Layer 2 Behaviour Graph. *(Current implementation: Layer 2 is provided by in-house `lib/utility_ai`, absorbed from big-brain and upgraded to Bevy 0.18.)* Pull spatial grid + query caching from bones-ai/rust-ants-colony-simulation. Pull basic needs/items/pawn entities from ryankopf/colony. Borrow update loop structure only from frederickjjoubert/bevy-colony-sim-game if needed. Discard all hardcoded AI rules and constants immediately.

- Phase 0 (complete before anything else): Prove Observer-mode drama first.  
  - Implement the exact minimal Transform schema (see example below) + solidification + hot-reload before any simulation logic.  
  - 8 drives only for Layer 1 NN: Hunger, Thirst, Fatigue, Curiosity, Social, Fear, Industriousness, Comfort (simple f32 chemical accumulation/decay on CPU for now).  
  - Global tick (C=∞) temporarily.  
  - Quest board as pure needs-based emergent market.  
  - Observer mode + basic Architect quest board.  
  - Success metric: After 10 sim-time hours with zero player input, at least one visible emergent story must appear (example: one pawn’s curiosity drive fires → discovers fire → teaches nearby pawn via social drive → campfire zone forms). Add simple logging so this can be verified.

- Later phases (only after Observer metric passes):  
  Phase 1: Full transform pipeline, epoch micro-versioning, contract validation (start simple, grow to SAT).  
  Phase 2: Introduce spatial charter + causal propagation (C=8–16).  
  Phase 3: Full pawn cognition + emergent economy.  
  GPU/1996-scale NN upgrade: After Observer works, expose NeuralNetworkComponent as a hot-reloadable transform. Scale to 64–256 drives and 4 GB total budget per colony (modern GPU inference via wgpu/Torch). Steve Grand’s original Norn scale is now trivial; use it for task matching, discovery, and vector embeddings later.

- Full Maxis sub-layers (SimEarth, SimLife, etc.), 4 GB NN, DuckDB vector index, narrative energy, multiplayer, and rendering polish are deferred until the transform + Observer foundation is proven solid.

---

## **Hardware Targets**

### Machines

| Component | Spec | Role |
| :---- | :---- | :---- |
| RTX 4080 | 16GB GDDR6X, ~165 TFLOPS FP16 | Game deployment target. Tiers 1–3 GPU batches run here. |
| A6000 | 48GB GDDR6, FP16/BF16 | Research/prototyping. Test large architectures without memory constraints, then distill to 4080. Not a co-processor. |
| AMD 8500G | 8c/16t Zen 4, RDNA3 iGPU | Biochemistry, game logic, Tiers 4–9, UI via iGPU. 4 cores: biochemistry + environment. 4 cores: genome + migration + game logic. |
| 64GB DDR5 | — | Hot cache, OS, agent state |
| 100GB NVMe | — | Population archive, save states, assets |

### VRAM Budget (RTX 4080 — 16GB)

| Allocation | Size | Purpose |
| :---- | :---- | :---- |
| All agent NNs | 4GB | Tier 1–4 active neural networks |
| → Tier 1 (sapient pawns) | ~1.5GB | 64 normal / 256 raid peak |
| → Tier 2 (adaptive) | ~500MB | ~1,000 active mammals/birds |
| → Tier 3/4 (reactive/reflex) | ~250MB | ~10,000 lower agents |
| → Headroom | ~1.75GB | Growth, compute buffers, migration workspace |
| Graphics / world / assets | 4GB | Rendering, sprites, textures |
| System reserve | 8GB | OS, driver, future |
| **Total** | **16GB ✓** | |

### System RAM (64GB)

| Allocation | Size | Purpose |
| :---- | :---- | :---- |
| Agent hot cache | 8GB | Dormant agent states, warm-swap (FP16, zstd compressed) |
| OS + game process | ~12GB | Conservative estimate |
| Free headroom | ~44GB | |

At Tier 1 scale (~3MB/pawn, ~600KB compressed), 8GB holds ~13,000 dormant sapients. Effectively unlimited for a local ecosystem.

### NVMe (100GB)

| Content | Size |
| :---- | :---- |
| Genome archive (compressed) | ~2GB (~33 million genomes at ~3KB compressed) |
| Agent save states (rolling history) | ~50GB |
| World state + scripted objects | ~5GB |
| Game assets | ~10GB |
| Dev headroom | ~33GB |

---

## **1000Hz Scheduling Grid**

The master clock runs at 1000 ticks per second. This is not a global tick — it is a **scheduling grid**. Think of it as Planck time for the simulation: fine-grained enough that any event can be slotted between any other event without needing fractional suffixes. We will never need more than 1000 distinct simultaneous event types; the grid ensures there is always room to insert a new event type between existing ones without renumbering.

Each system runs at its natural frequency as an integer divisor of 1000. Nothing updates simultaneously. See `03_engine_architecture §3.8` for how this reconciles with the causal propagation model.
