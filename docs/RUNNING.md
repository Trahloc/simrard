# Running Simrard (Observer Mode)

## How to run

From the project root:

```bash
cargo run
```

A window opens. You're in **Observer mode**: the sim runs with no player input. Two chunk clusters are visible:

- **Chunk (0,0)** (bottom-left): green = pawns, orange = food, blue = water, brown = rest spot.
- **Chunk (10,10)** (top-right): same layout.

Sim time is **decoupled from frame rate**. At "1x" the sim runs at 10 causal ticks per second so you can watch at a normal pace.

## Time scale (keys)

| Key | Action |
|-----|--------|
| **]** | Speed up (×1.5, max 20×) |
| **[** | Slow down (÷1.5, min 0.1×) |
| **1** | Reset to 1× |
| **2** | Set to 2× |
| **P** | Pause / Unpause (toggle 0× ↔ 1×) |

- **1×** = 10 sim ticks per second (recommended for watching).
- Pause (0×) to inspect the window; unpause with **P** again.

## What you see

- **Green** = Pawns (spread slightly in each chunk).
- **Orange** = Food (portions; disappears when depleted).
- **Blue** = Water (same).
- **Brown** = Rest spot.

Drive decay and threshold events (hunger, thirst, fatigue) run every 10 causal ticks. Pawns eat, drink, or rest based on utility AI; the charter ensures only one pawn can use a resource in a chunk at a time. Console logs (stderr) show causal events and lease grants/denials when running from a terminal.
