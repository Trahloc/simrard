# **PART 1 — DESIGN PHILOSOPHY**

## **Preamble**

Simrard would not exist without three people.

**Jonathan Gorard** — the mathematician whose rigorous work on the Wolfram Physics Project demonstrated that the universe's structure might emerge from a tiny set of hypergraph rewriting rules applied recursively. No designer. No intent. Just rules and time. The game is named in his honor.

**Stephen Wolfram** — who had the audacity to ask whether physics itself was a kind of cellular automaton, and built the project that let Gorard prove it might be right. The philosophical permission slip for this whole endeavor.

**Stephen Grand** — who in 1996 built *Creatures*, a commercial game where synthetic animals lived, learned, and evolved using biologically inspired neural networks and artificial biochemistry. He proved that simple biological rules generate surprising life-like behavior. His paper is inspiration, not bible — but without it this design has no spine.

The synthesis: **Gorard gives us the bottom of the world. Grand gives us the top. Everything in between is the same idea applied at different scales — simple rules, no author pulling strings, surprising outcomes.**

---

## **1.1  Core Statement**

| The Game In One Sentence A true simulation — simplified but honest — where the player is a gardener of systems, not a commander of units. Closer to Conway's Game of Life than The Sims. |
| :---- |

The game belongs to the jazz era of gaming: well-defined harmonic structures (core systems) on top of which improvisation (mods, player action, emergent behavior) plays freely. The standard and the improvisation use the same notation. The developers have no privileged access the modder doesn't have.

## **1.2  The Maxis Lineage**

Will Wright's pre-EA Maxis catalogue is the spiritual ancestor. Each old Maxis title maps to a simulation layer that exists simultaneously at different zoom levels. The Tier column maps each layer to its position in the Simrard tier stack (see §1.4).

| Game | Simulation Layer | Tiers |
| :---- | :---- | :---- |
| SimEarth | Geology, climate, biome generation, long time-scale forces | 8–9 |
| SimLife | Ecology, food webs, species adaptation, population dynamics | 5–7 |
| SimAnt | Sub-colony civilisations (the ant colony under your base has its own politics) | 3–4 |
| SimCity | Settlement infrastructure, economic zones, logistics networks | 1–2 infrastructure |
| Creatures | Individual pawn & animal cognition (Steve Grand's neural biochemistry lineage) | 1–4 cognition |
| RimWorld | Colony management, narrative drama, resource pressure | 0–1 |

The player does not choose a zoom level. They move between them. The hero pawn digs into the earth and finds SimEarth. A microscope finds SimLife. These layers are not lore — they are the knowledge system.

## **1.3  Player Relationship To The Simulation**

The player has four modes, all diegetic. No menu breaks. All transitions happen inside the world.

| Mode | Analogy | Player Action |
| :---- | :---- | :---- |
| **Observer** | *Aquarium / DF* | Watch. No hero. No intervention required. Valid gameplay. |
| **Architect** | *Quest Board / RimWorld* | Post intent via physical quest board. Pawns self-select by skill/drive. 'Go check this out' is weighted suggestion, not command. |
| **Hero (Riding)** | *Creatures / RPG* | Inhabit one pawn for extended period. World continues without you. Hero NN identical to everyone else's — you influence it, not override it. |
| **Discovery (SimX)** | *Documentary zoom* | Pawn or hero digs into a deeper simulation layer. Research is done by finding things, not filling progress bars. |

| 📝  EXPLORE — Philosophy |
| :---- |
| →  Narrative energy / god power (Black & White model) — player earns intervention capacity through colony success. Spending it on quest rewards (known items: cheap) vs. introducing novel catalyst items (expensive, triggers discovery cascade). How does this avoid feeling like a resource grind? |
| →  Hero death as chapter boundary, not failure state. Colony persists. New hero selected or new game started. What determines narrative continuity across hero deaths? |
| →  Screensaver legitimacy — the Observer mode must generate genuinely interesting drama with zero player input. This is the hardest design commitment and the first thing to prototype. |

---

## **1.4  The Tier Stack**

Everything in Simrard lives at one of eleven tiers. Each tier is a transformer of the tier below it — simple rules applied to the output of the layer beneath.

```
Tier 0  — Player      Outside the system. The curator, not a participant.
Tier 1  — Sapient     Pawns. Social, linguistic, episodic memory. (Norn equiv.)
Tier 2  — Adaptive    Mammals, birds. Full within-lifetime learning.
Tier 3  — Reactive    Reptiles, fish. Drive-based, shallow learning, individual recognition.
Tier 4  — Reflex      Insects. Fixed or near-fixed wiring. Habituation only.
Tier 5  — Vegetable   Plants. Growth, uptake, energy transduction.
Tier 6  — Fungal      Mycelium. Decomposition, nutrient redistribution networks.
Tier 7  — Chemical    Reaction dynamics. The trophic chemistry layer.
Tier 8  — Mineral     Stable inorganic structures. Soil, stone, substrate.
Tier 9  — Energy      Abiotic flux. Light, heat, radiation. The ultimate driver.
Tier 10 — Hypergraph  Wolfram/Gorard rewriting rules. Physics itself. Simrard's namesake.
```

**The chain reads upward:** Minerals absorb energy. Chemical reactions occur in mineral substrate. Fungi redistribute chemical products of dead matter back into soil. Plants transduce chemical energy into organics. Insects eat plants. Reactive animals eat insects and plants. Adaptive animals eat across tiers. Sapients eat everything and reshape the environment deliberately.

**Trophic chemistry is real here.** Base chemicals originate at the bottom — algae fixing nutrients from water and light, fungi pulling minerals from dead matter. Small things concentrate those chemicals. Bigger things concentrate further. A pawn's nutrient deficiency isn't a pawn problem — it might be a soil chemistry problem three trophic levels down. No event is scripted to cause it. The food web just works that way if the rules are right.

**The world as superorganism:** The full tier stack is, in aggregate, a single energy-transformation machine. Tier 9 drives Tier 8 drives Tier 7 and so on up to Tier 1. The pawns are what happens at the top of a cascade that started with a photon hitting soil. Every tier is a "polyp type" in a planetary siphonophore — specialized, interdependent, individually simple, collectively alive.

---

## **1.5  The Design Test**

For every rule added to the system: can you predict all of its second-order interactions? If yes, it's probably too specific — you're scripting a behavior rather than enabling emergence. If you can predict the first-order effect but the second-order interactions are genuinely surprising to you as the designer, that's the sweet spot.

If a rule exists to *cause* a specific desired behavior, it probably shouldn't exist. If a rule exists to *allow* behavior to emerge, it belongs.

Pack hunting should not be a behavior. It should fall out of: wolves have a social drive, a fear drive, and a hunger drive; lone wolves are afraid of large prey; wolves emit a chemical that raises nearby wolves' hunger and suppresses their fear. Nobody wrote "hunt in packs." The world did.

---

## **1.6  The Receptor/Emitter Principle**

The most important implementation rule in the system, elevated here to design philosophy because it is the mechanism that keeps every tier emergent.

**Never write a cross-tier interaction as an event threshold conditional.** Any time you find yourself writing `if concentration > threshold then trigger X`, you have accidentally scripted an event. Replace it with a receptor whose gain makes the interaction naturally proportional.

```
❌  if soil_nutrient_B > 50 then plant.grow()       ← scripted event
✓   plant.growth_rate = receptor(soil_nutrient_B, gain=0.8)  ← proportional response
```

High concentration → receptor fires strongly → uptake fast. Low → uptake slow. Zero → nothing happens. The depletion cascade is just what receptor gain × near-zero concentration looks like across a region over time. You didn't write a famine — the world wrote it.

This principle applies to every cross-tier boundary. It is not a guideline. It is the mechanism that separates emergent simulation from scripted theater. The moment a cross-tier interaction acquires a hardcoded event threshold it becomes a script.

Exception: receptor `noise_floor` is allowed as a numeric stability guard to prevent slow rest-state drift from near-zero concentrations. It is not a behavior-control parameter and should remain close to zero.

The receptor and emitter are implemented as first-class data primitives with versioned, named fields in the transform system (see `02_mod_and_data_architecture §2.8`). The full field specification is in `04_agent_cognition §4.5`.
