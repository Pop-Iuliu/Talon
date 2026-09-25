# Talon

Talon is a directed greybox fuzzer prototype. It steers mutation-based fuzzing
toward a named target function in a binary, using static CFG distances instead
of relying on plain coverage guidance.

The repository is a research prototype with three moving parts:

- `graph_engine.py`: builds a CFG of the target function with angr, recovers
  which basic block calls each `hit_block(N)`, and computes hop-count
  distances from every instrumented block to the target. The result is a
  versioned JSON distance map keyed by the `hit_block` IDs the fuzzer
  observes in its signal map.
- `Fuzzer/`: a LibAFL 0.16 fuzzer with a `DirectedDistanceScheduler` that
  combines the distance map with simulated-annealing style cooling.
- `Tests/if_nest.c`: a toy C target, instrumented by hand with `hit_block(id)`
  calls, compiled to `Tests/libif_nest.so` and loaded into the fuzzer in
  process.

An ID names the block containing the `hit_block(N)` call, which is the block
right before the branch or store that the ID is meant to name: in the toy
target, ID 103 sits in block `0x119f`, while the crashing store is in the
successor block `0x11a9`.

## Pipeline

```
if_nest.c ──gcc──> libif_nest.so ──angr/graph_engine.py──> distances.json
                                                              │
                              Fuzzer (LibAFL) <───────────────┘
                                       │
                                 crashes/
```

## Prerequisites

- Rust 1.94.1 (pinned in `rust-toolchain.toml`; rustup installs it on demand)
- Python 3 with the pinned dependencies: `pip install -r requirements.txt`
- `gcc` and `cargo` on `PATH`

## Quick start

```
./build_and_run.sh
```

The script compiles the target, generates a fresh `distances.json` from the
just-built binary, and runs the fuzzer with `cargo run --release`. The crash
in the toy target is found in a few seconds and saved under `Fuzzer/crashes/`.

To run the pieces manually:

```
gcc -shared -fPIC -O0 -fno-inline Tests/if_nest.c -o Tests/libif_nest.so
python3 graph_engine.py Tests/libif_nest.so target_function --target-id 103 --out Fuzzer/distances.json
cd Fuzzer && cargo run --release
```

The target is selected with `--target-id N` (a `hit_block` id) or
`--target-addr 0x…` (a basic block address). The output has the schema

```
{"version": 1, "target_id": 103, "map_size": 4096, "distances": {"100": 6, ...}}
```

The Rust loader rejects a wrong version, a mismatched map size and IDs beyond
the map size.

The fuzzer takes flags: `--distances PATH`, `--cooling-secs SECS`,
`--seed N` (random by default, printed at startup), `--crashes-dir DIR`, and
`--scheduler {directed,queue,rand}` for the baselines. Mutations per seed go
through a power schedule (`PowerMutationalStage`): near and cold seeds get
more mutations, and the baselines get a fixed budget.

## Benchmark

```
./bench.sh
```

Builds the harder `Tests/if_nest_hard.c` target (ten magic bytes plus decoy
branches), runs 10 paired trials per scheduler with a 60 s budget, and prints
time-to-exposure statistics (median, Vargha-Delaney A12, Mann-Whitney U).
The current numbers and the reading of the negative result are in
`docs/evaluation.md`.

## Known limitations

- Distances are intra-procedural: targets inside a callee do not attract
  seeds yet.
- ID recovery depends on the target being compiled with the pinned flags
  (`-O0 -fno-inline`); other optimization levels may move argument setup
  across block boundaries.
- Hand-written `hit_block` instrumentation instead of a real coverage pass.
- The benchmark shows no significant time-to-exposure gain for the directed
  scheduler over the queue and rand baselines; see `docs/evaluation.md`.
- Single-threaded, in-process fuzzing: the campaign ends at the first crash.
