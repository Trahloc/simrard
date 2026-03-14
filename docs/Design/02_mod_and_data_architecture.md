# **PART 2 — MOD & DATA ARCHITECTURE**

The single most important constraint: the developers have no privileged access the modder doesn't have. Everything in the game — core content, mechanics, behaviours — is expressed as a transform in the same language available to mods. This is not a goal; it is a hard invariant enforced by convention and architecture.

## **2.1  Everything Is A Transform**

The base game is the identity layer — the first transform applied to nothing. A mod is a subsequent transform. There is no separate 'game code path' and 'mod code path'.

| ∅  →  \[core transforms\]  →  base game state |
| :---- |
| base game state  →  \[mod A transforms\]  →  modded state A |
| modded state A   →  \[mod B transforms\]  →  final state |

## **2.2  Semantic Differential Primitives**

Transforms are typed and semantic, not raw file patches. Operations compose cleanly. Raw patches conflict.

| target:    entity/pawn/colonist |
| :---- |
| operation: modify |
| path:      needs.food.depletionRate |
| value:     0.8x          \# relative modifier, not absolute replacement |
| contract:  '\>=epoch:2847' |

The solidification step merges transforms into a cached build artifact — analogous to a compiled binary. Source of truth is always the transform stack. The solidified artifact is derived and cached, never stored as canonical.

| Canonical:   \[base\] \+ \[mod diffs\]     \# small, versioned, diffable |
| :---- |
| Derived:     solidified artifact       \# cached, rebuilt when stale |
| Runtime:     loaded from derived cache \# zero merge overhead at launch |

## **2.3  Epoch-Based Micro-versioning**

Standard semver (major.minor) is owned by humans. Micro-version (the third digit) is owned by the automated migration system. Humans never touch it directly.

Each feature has an epoch — a monotonically increasing causal sequence number, not wall time. Epochs express causal ordering, not timestamps.

| feature:    pawn.needs.hunger.depletionRate |
| :---- |
| introduced: epoch:2847392     \# unique, never recycled |
| supersedes: epoch:1923847     \# what it replaced |
| status:     active            \# active | deprecated | tombstoned |

A mod declares: 'I depend on pawn.needs.hunger at epoch \>=1923.' The solver asks: did anything causally incompatible change between epoch 1923 and the current epoch in that feature's subtree? Not 'is the version number different?' — 'is there a causal conflict?' Everything else gets benefit of the doubt.

## **2.4  Naming Policy — Hyrum's Law Enforcement**

Once a name is exposed it is contracted forever. The toolchain enforces this, not convention.

* Renames always create an alias — both old and new name work transparently
* Old names are tombstoned at the next major version, never sooner
* Tombstoned names are blacklisted permanently — never reused, ever
* The namespace registry keeps the graveyard forever (zero runtime cost, infinite semantic protection)
* Breaking a name intentionally requires an explicit deprecation notice to affected mod maintainers with a recommended migration patch

## **2.5  Contract Layer & SAT Solver Validation**

Before solidification, the solver validates the full transform stack. It is doing two things simultaneously:

* Dependency resolution — topological sort of transforms respecting declared dependencies
* Contract validation — each transform's declared behavioral assumptions are satisfied by what is below it in the stack

Failures are precise: 'Mod X transform food.depletionRate assumes the needs subsystem has property depletionRate of type float. In the current stack, Mod Y has removed that property. Incompatibility between Mod X v2.3 and Mod Y v1.1.' Not a crash. A diagnostic.

## **2.6  Evergreen Mods — Community As CI Suite**

This is the most strategically important consequence of the architecture: the mod ecosystem is a continuous integration suite the developers didn't have to write.

Every mod that declares its transforms and contracts is implicitly writing a test. When core changes, the migration system runs against all registered mods:

| Core change committed |
| :---- |
|   → migration system runs against all registered mods |
|   → mechanical migration (alias, type widen, default add): |
|         apply automatically, bump mod micro-version |
|         mod author notified: 'your mod was auto-migrated to 1.1.4' |
|   → semantic migration required: |
|         flag for author, generate suggested patch with confidence score |
|         mod sits in 'compatibility limbo', not 'broken' |
|   → genuinely mutually exclusive: |
|         hard incompatibility, precise error, no auto-migration |

Breaking a top-100 mod is a signal as meaningful as breaking a unit test. Before any release, the full known-mod test suite runs. Intentional breaks get a heads-up to the mod maintainer with a recommended patch that works in both current and future versions.

## **2.7  Hot Reload**

Because transforms are the source of truth and the solidified artifact is a derived cache, mod changes at runtime are:

* Invalidate affected portions of the solidified cache
* Re-run the solver on the changed transform stack
* Re-solidify the affected portion
* Reload affected derived state through the dataflow graph

This enables live mod iteration without restarting. For AI-assisted game development this is critical — generate a transform, inject it, observe the result in seconds, iterate. This must be treated as a hard design invariant, not a nice-to-have.

## **2.8  Biochemistry Primitives as Transform Targets**

The biochemistry system's four object types — chemicals, emitters, receptors, and reactions — are first-class transform targets. A mod that adds a new emotional response to a stimulus does so by declaring a new receptor or emitter, not by patching logic. This is the mechanism that makes emergent behavior moddable without opening engine internals.

### Emitter Transform Fields

An emitter observes a locus byte in a system object and produces a chemical when it changes.

| Field | Type | Description |
| :---- | :---- | :---- |
| `source_system` | string | Organ/tissue identifier (`brain`, `gut`, `muscle`, etc.) |
| `source_subsystem` | string | Sub-tissue selector |
| `reads_field` | string | Namespaced locus path being observed |
| `emits_chemical` | string | Named chemical produced |
| `threshold` | f32 | Minimum locus value before emission begins |
| `rate` | f32 | Emission rate (exponential dynamics) |
| `gain` | f32 | Scalar multiplier on emission amount |
| `applicator` | enum | `add` \| `set` |

### Receptor Transform Fields

A receptor monitors a chemical concentration and writes a locus byte.

| Field | Type | Description |
| :---- | :---- | :---- |
| `target_system` | string | Organ/tissue identifier |
| `target_subsystem` | string | Sub-tissue selector |
| `write_field` | string | Namespaced locus path to modify |
| `monitors_chemical` | string | Named chemical tracked |
| `threshold` | f32 | Concentration below which receptor is silent |
| `nominal` | f32 | Expected concentration at normal state |
| `gain` | f32 | Scalar multiplier on output signal |
| `applicator` | enum | `replace` \| `modulate` |

### Reaction Transform Fields

| Field | Type | Description |
| :---- | :---- | :---- |
| `reactant_1` | string + proportion | Chemical consumed |
| `reactant_2` | string + proportion | Optional second reactant |
| `product_1` | string + proportion | Chemical produced |
| `product_2` | string + proportion | Optional second product |
| `rate` | f32 | Reaction rate (concentration-dependent, exponential) |

All three primitives follow the same naming, versioning, epoch, and tombstone conventions as §2.4. Example path: `biochemistry.receptor.hunger_protein.gain`. This is a namespaced, versioned field — renames create aliases, tombstoned names are blacklisted permanently.

### Genome Bytes as Transform-Layer Content

SVRules compile to pre-built kernels at gene-expression time (birth, puberty). The compiled kernel is the solidified artifact. The genome bytes are the canonical source.

```yaml
target:    genome/lobe/7/svrule
operation: modify
value:     0x2A                   # new opcode byte
contract:  '>=epoch:4109'
```

The solidification step re-compiles the affected SVRule kernel when the genome transform is applied, exactly as it re-evaluates any other cached artifact. Hot-reload at pawn birth applies normally. The gene expression pipeline is a transform: it takes genome bytes as input and produces compiled kernel + lobe parameter tables as output. Both the source and the derived artifact pass through the transform/solidification system — there is only one canonical data source.

| 📝  EXPLORE — Mod Architecture |
| :---- |
| →  Schema migration tooling UX — what does a mod author actually see when the system auto-migrates their mod? How are confidence scores communicated? |
| →  Mod signing / trust tiers — anonymous community mods vs. verified studio mods. Does the contract layer change based on trust tier? |
| →  AI-generated transforms — the transform schema is the interface. An LLM that understands the schema can generate valid transforms from natural language. What does that workflow look like in practice? Implications for personalised game generation. |
| →  Mod composition UI — how does a player who is not a modder configure their personal transform stack? Is there a 'playlist' metaphor? |
