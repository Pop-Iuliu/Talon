# Talon

Talon is a directed greybox fuzzer prototype. It steers mutation-based fuzzing
toward a named target function in a binary, using static CFG distances instead
of relying on plain coverage guidance.

The repository is a research prototype with three moving parts:

- `graph_engine.py`: builds a CFG of the target function with angr and computes
  shortest-path distances from every basic block to the target block. The
  result is a JSON distance map.
- `Fuzzer/`: a LibAFL 0.16 fuzzer with a `DirectedDistanceScheduler` that
  combines the distance map with simulated-annealing style cooling.
- `Tests/if_nest.c`: a toy C target, instrumented by hand with `hit_block(id)`
  calls, compiled to `Tests/libif_nest.so` and loaded into the fuzzer in
  process.

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
gcc -shared -fPIC Tests/if_nest.c -o Tests/libif_nest.so
python3 graph_engine.py Tests/libif_nest.so target_function --out Fuzzer/distances.json
cd Fuzzer && cargo run --release
```

## Known limitations

- The "directed" scheduling is not wired up yet: distances are keyed by block
  addresses while the fuzzer observes hand-picked `hit_block` IDs, so every
  lookup misses. Plain coverage guidance finds the crash on its own. See
  `plan.md`, Sprint 2 (Jnana).
- The target block is chosen with a heuristic (first end node), which picks the
  function epilogue rather than the crash site.
- Single-threaded, in-process fuzzing: the campaign ends at the first crash.
- Hand-written `hit_block` instrumentation instead of a real coverage pass.
