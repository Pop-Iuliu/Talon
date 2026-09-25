# Talon Sprint Plan: Aviarist, Jnana, Vinosities

> Three one-week sprints, one per word. Talon is a claw, so the words map onto
> the project like this:
>
> | Word | Meaning | Sprint theme |
> |---|---|---|
> | **aviarist** | someone who keeps an aviary | **Keep the nest clean**: repo hygiene, a single build pipeline, CI. The target is literally `if_nest`. |
> | **jnana** | (Sanskrit) true knowledge | **Know where the bug is**: correct distances, explicit target selection, one shared ID space, tests. |
> | **vinosities** | the qualities of a wine | **Age seeds at a controlled temperature, then taste them**: live simulated annealing, a power schedule, a benchmark against baselines. |

- **Planned:** 2026-09-25
- **Cadence:** 1-week sprints, solo developer
- **Assumed velocity:** about 15 ± 3 points per sprint. This is a guess; re-estimate after Sprint 1.
- **Sprints:**
  - Sprint 1 (Aviarist): Mon 2026-09-28 to Fri 2026-10-02
  - Sprint 2 (Jnana): Mon 2026-10-05 to Fri 2026-10-09
  - Sprint 3 (Vinosities): Mon 2026-10-12 to Fri 2026-10-16

---

## 0. Where the code stands now (the reason for this plan)

Talon is a directed greybox fuzzer prototype, about 350 lines of hand-written code:

- **`graph_engine.py`**: Python, uses angr and networkx to build a CFG and compute distances to a target block.
- **`Fuzzer/`**: Rust, LibAFL 0.16.1, with a custom `DirectedDistanceScheduler`.
- **`Tests/if_nest.c`**: a toy C target, instrumented by hand with `hit_block(id)` calls.
- **`build_and_run.sh`**: glues the three together.

The pipeline runs and finds the crash in about 4 s. **But the "directed" part currently does nothing.** Plain coverage guidance finds the crash on its own. Specifically:

| # | Defect | Where |
|---|---|---|
| D1 | Distances are keyed by **hex block addresses** (`"0x114c"`), but the scheduler looks them up by **decimal `SIGNALS` index** (`"100"`). Every lookup misses and falls back to 1.0. | `graph_engine.py:47`, `Fuzzer/src/scheduler.rs:52` |
| D2 | Two different ID spaces: coverage uses hand-picked `hit_block` IDs (100 to 103) and the graph uses block addresses. Nothing maps one to the other. | `Tests/if_nest.c:11-20` |
| D3 | The target block is a guess: "first node with no successors". It picks the function epilogue (`leave; ret`), not the crash. | `graph_engine.py:38-39` |
| D4 | `seed_distances` is never filled in (`on_add` is a stub, `on_evaluation` is missing), so `next()` always returns the newest entry. | `scheduler.rs:77-79, 90-91` |
| D5 | `calculate_seed_distance` and `calculate_energy` are never called. Also, `calculate_energy` expects a normalised distance in [0,1] but would receive a raw hop count, so **far seeds would get *more* energy**: (1-3)² > (1-0)². | `scheduler.rs:46-70` |
| D6 | The fuzzer reads **stale artifacts**. The script writes `./distances.json`, then `cd Fuzzer` and the scheduler reads `Fuzzer/distances.json` (built from an older target). `build.rs` links the stale root `libif_nest.so`. The scheduler ignores its `path` argument and hard-codes the file name. | `build_and_run.sh:36,47`, `build.rs:2-8`, `scheduler.rs:23` |
| D7 | `graph_engine.py` exits with 0 when it fails, so the script carries on with stale distances. | `graph_engine.py:19-21, 27-29` |
| D8 | `set_current_scheduled` is a no-op, so the current corpus entry and the parent IDs are never set, and the splice mutators can pair a testcase with itself. | `scheduler.rs:107-113` |
| D9 | The campaign ends at the first crash (non-restarting `SimpleEventManager` plus exit 139), and it ends before the 5 s cooling window finishes, so the temperature never matters. | `main.rs:37,45` |
| D10 | There are no tests, no CI, no README, no LICENSE and no Python dependency manifest. Generated binaries and JSON files are tracked (3× `libif_nest.so`, 2× `distances.json`). | repo root |

Other clean-up items:
- Unused `RawDistances` struct, `serde` import and `BufReader` (`main.rs:32-33`, `scheduler.rs:20-22`).
- Unused `tui_monitor` feature (`Cargo.toml:7`).
- Deprecated `InProcessExecutor::new` (`main.rs:71`).
- `SIGNALS` is cleared twice per execution (`main.rs:64`; `StdMapObserver` already resets it).
- Error messages in Romanian (`scheduler.rs:21,26,84`).
- The `fuser -k 1337/tcp` / `pkill -9` leftovers in the script (`build_and_run.sh:8-9`).

> **Note:** `.omo/plans/talon-directed-sprint.md` is an earlier AI-generated sprint
> plan that was never started or reviewed. This plan replaces it and fixes four gaps in it:
> 1. It never fixed target selection (D3).
> 2. It missed that each `hit_block(N)` call sits in the block *before* the branch it is named after.
> 3. Its "cooling floor reached" proof can't happen, because the process exits after about 4 s.
> 4. It kept the trivial target, which cannot show that directed fuzzing helps.

---

## Definition of Done (every item)

- `cargo build --release` gives **zero warnings**, and `cargo clippy -- -D warnings` passes.
- `cargo fmt --check` passes. Python code passes `ruff` (or at least `python -m py_compile`).
- New logic has tests, and CI is green (from Sprint 1 on).
- No generated artifacts (`*.so`, `distances.json`, `crashes/`) are committed.
- The README or docs are updated if behaviour or usage changed.
- The change is merged to `main` through a PR, even when working solo, to keep history reviewable.

---

## Sprint 1 — Aviarist: keep the nest clean

**Goal:** a fresh clone builds and runs end to end with one command, uses *freshly generated* artifacts only, and CI checks every push.

| ID | Story | Pts | Acceptance criteria |
|---|---|---|---|
| AV-1 | **Stop tracking generated files** | 1 | `git rm --cached` removes `libif_nest.so`, `Fuzzer/libif_nest.so`, `Tests/libif_nest.so`, `distances.json` and `Fuzzer/distances.json`. `.gitignore` covers `*.so`, `distances.json` and `.omo/`. `git status` stays clean after a full run. |
| AV-2 | **One source of truth for artifacts** (fixes D6, D7) | 3 | `graph_engine.py` accepts `--out PATH` and exits 1 on every error path. `build.rs` links from `../Tests` and uses `rerun-if-changed=../Tests/libif_nest.so`. `DirectedDistanceScheduler::new` reads the `path` it is given. The script writes distances to the exact path the fuzzer reads. |
| AV-3 | **Harden `build_and_run.sh`** | 2 | Uses `set -euo pipefail`. Uses `${PYTHON:-python3}` instead of the Anaconda-only `python`. Runs `cargo run --release`. The `fuser -k 1337/tcp`, `pkill -9` and `clear` lines are removed. Works from any working directory (`cd "$(dirname "$0")"`). |
| AV-4 | **Warning-free, English, non-panicking core** | 3 | Remove `RawDistances`, the unused `serde` import and dependency, the unused `BufReader`, and the `tui_monitor` feature. The loader returns `Result<_, Error>` instead of panicking. All messages are in English. Migrate to `InProcessExecutor::builder()` and drop `#[allow(deprecated)]`. Remove the manual `SIGNALS` clear at `main.rs:64`. |
| AV-5 | **Reproducible environment and README** | 2 | `requirements.txt` pins `angr==9.3.4` and `networkx==3.3`. `rust-toolchain.toml` is added. The README covers what Talon is, prerequisites, a quick start, a pipeline diagram and known limitations. **Owner decision:** choose a LICENSE; the repo is public with no license. |
| AV-6 | **CI skeleton** (GitHub Actions) | 3 | Job `rust`: compile `Tests/libif_nest.so` first (build.rs needs it), then `fmt --check`, `clippy -D warnings`, `build`, `test`. Job `python`: `pip install -r requirements.txt` with caching, then a smoke run of `graph_engine.py` on the compiled target that checks the output JSON is non-empty. |
| AV-7 | **Prune stale branches and worktrees** (owner confirms first) | 1 | Remove `fix/cfg-distance-engine` (local and remote, already merged), the `agents/help-me-fix-describe-the-bug-in-this` branch and worktree, and the orphan `refs/agents/*`. |
| | **Total** | **15** | |

**Sprint 1 demo:** `git clone` → `pip install -r requirements.txt` → `./build_and_run.sh` finds the crash with a release build, and the logs show the distances file that was *just* generated being loaded. The CI badge is green.

**Retro questions:** Was 15 points realistic? How long did a full CI run take, given how heavy angr is?

---

## Sprint 2 — Jnana: know where the bug is

**Goal:** the distances point at the bug the user *names*, use the same ID space the fuzzer observes, and tests prove it.

| ID | Story | Pts | Acceptance criteria |
|---|---|---|---|
| JN-1 | **Explicit target selection** (fixes D3) | 3 | `graph_engine.py` takes `--target-id N` (a `hit_block` ID) or `--target-addr 0x…`. The "first end node" heuristic is deleted. If the target is not in the graph, it exits 1 with a clear message. |
| JN-2 | **Map `hit_block` IDs to basic blocks** (fixes D1, D2) | 5 | For each block, find `call hit_block` and recover the constant argument (for example `mov edi, 0x64`) via angr/capstone. Emit distances keyed by **decimal `SIGNALS` index**. Document the rule that an ID stands for the *block containing the call*, which is the block before the branch it names; ID 103 sits in `0x119f`, while the crashing store is in `0x11a9`. Blocks without an ID do not appear in the output. |
| JN-3 | **Versioned distance schema and Rust validation** | 2 | Format: `{"version":1,"target_id":103,"map_size":65536,"distances":{"100":3,…}}`. The Rust loader rejects a wrong version, rejects keys ≥ `MAP_SIZE`, and logs how many distances it loaded and the target ID. |
| JN-4 | **Testable graph engine with one reverse BFS** | 3 | Split `main()` into `build_graph`, `map_ids` and `distances_to`. Replace the per-node shortest-path loop with `nx.single_source_shortest_path_length(G.reverse(copy=False), target)`. Add pytest unit tests on hand-built graphs, plus a golden test on `if_nest.c` compiled with pinned flags (`-O0 -fno-inline`). |
| JN-5 | **Scheduler unit tests** | 3 | Inject time (pass `elapsed`, or a `Clock` trait) so `current_temperature` can be tested deterministically: 1.0 at t=0, the 0.01 floor after cooling, strictly decreasing. `calculate_seed_distance`: known hits give the expected mean, and no hits give the fallback. The tests run in CI. |
| JN-6 | *(stretch)* **Inter-procedural distance** | 5 | Use the call graph plus the AFLGo harmonic-mean formula so targets in a callee still attract seeds. Only needed once targets have more than one function. |
| | **Committed / stretch** | **16 / 5** | |

**Sprint 2 demo:** `graph_engine.py Tests/libif_nest.so target_function --target-id 103 --out Fuzzer/distances.json` outputs keys `100`–`103` with `"103": 0`. The fuzzer logs a **matched-distance count greater than 0** for real seeds (0 before this sprint). `pytest` and `cargo test` pass in CI.

**Risk to watch:** recovering the constant argument depends on how the target was compiled. That is why JN-4 pins the compiler flags. If the recovery is too fragile, fall back to having `hit_block` IDs emitted into a sidecar table at compile time, for example with a macro that records `(id, __LINE__)`.

---

## Sprint 3 — Vinosities: age seeds at a controlled temperature, then taste them

**Goal:** simulated annealing actually drives the fuzzer, and a fair benchmark on a target that is *hard enough* shows whether directed scheduling beats the baselines. A negative result, reported honestly, still counts as done.

| ID | Story | Pts | Acceptance criteria |
|---|---|---|---|
| VI-1 | **Record and normalise seed distances** (fixes D4, D5) | 5 | Implement `on_evaluation`: read the `"signals"` observer and compute the mean distance over the IDs that were hit. In `on_add`, attach it to the testcase as `SeedDistanceMetadata`. Normalise to [0,1] using the running min/max across the corpus (as AFLGo does) *before* calling `calculate_energy`. Add a unit test showing near seeds get more energy than far seeds when T is low. |
| VI-2 | **Annealed selection and correct bookkeeping** (fixes D8) | 3 | `next()` explores (uniform random) with probability ≈ T and otherwise exploits, weighting seeds by 1−d̂, so it is no longer always "newest" or always argmin (both starve seeds). `set_current_scheduled` and `on_add` set the current entry and parent ID the way `RandScheduler` does. |
| VI-3 | **Energy through a power stage** | 5 | Implement a `TestcaseScore` that uses `calculate_energy`, and replace `StdMutationalStage` with `PowerMutationalStage`. **Spike first (timebox: half a day).** If LibAFL's generic bounds get in the way, fall back to a small custom stage that runs `energy` mutations per seed. |
| VI-4 | **Fuzzer CLI** | 2 | Flags: `--distances`, `--cooling-secs` (the 5 s default is far too short for real targets), `--seed` (random by default, printed at startup so runs can be reproduced), `--crashes-dir`, and `--scheduler {directed,queue,rand}` for baselines. |
| VI-5 | **Benchmark** | 3 | Restore the harder `TALON\x01\xDE\xAD\xBE\xEF` target from commit `3dc07f4` as `Tests/if_nest_hard.c`, and add *decoy* branches full of coverage that lead away from the bug. A `bench.sh` script runs at least 10 trials per scheduler with release builds and a fixed timeout, recording **time to exposure (TTE)**. `docs/evaluation.md` reports the median TTE, Vargha–Delaney Â12 and Mann–Whitney U. |
| VI-6 | *(stretch)* **Survive the first crash** (fixes D9) | 3 | Keep fuzzing after an objective, using a restarting manager or `InProcessForkExecutor`; decide which in a short spike. Add `TimeoutFeedback` to the objective so hangs are saved. Persist the scheduler state (start time, min/max distance) in state metadata so a restart doesn't reset the temperature. *Not needed for VI-5, because TTE only needs the first crash.* |
| | **Committed / stretch** | **18 / 3** | |

> 18 points is above the assumed velocity. If Sprint 1 comes in slow, **cut VI-3 first.**
> Annealed *selection* (VI-1 and VI-2) is enough on its own to benchmark, and the
> power stage can move to the backlog.

**Sprint 3 demo:** `bench.sh` produces a table like this:

| Scheduler | Median TTE | Â12 vs directed | p |
|---|---|---|---|
| directed | … | — | — |
| queue | … | … | … |
| rand | … | … | … |

It also produces a temperature/energy trace over one campaign, showing the cooling actually happening.

---

## Dependencies

```
AV-1 ─┐
AV-2 ─┼─> JN-1 ─> JN-2 ─> JN-3 ─> VI-1 ─> VI-2 ─> VI-3
AV-3 ─┘            │                 │
AV-4 ──────────────┴─> JN-5 ─────────┘
AV-5, AV-6 ─> (CI runs every later test)          VI-4 ─> VI-5
```

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| LibAFL generic bounds make `PowerMutationalStage`/`TestcaseScore` painful | High | Medium | Timeboxed spike (VI-3); custom-stage fallback; VI-3 is the first item to cut |
| angr's ID recovery breaks with other compiler flags | Medium | High | Pin the flags, add a golden test (JN-4), sidecar-table fallback |
| angr install makes CI slow or flaky | Medium | Low | Pip cache and a separate Python job; the Rust job doesn't need angr |
| Directed scheduling doesn't beat the baselines | Medium | Low | That is still a valid result: report it and tune the cooling time and decoys |
| Single developer (bus factor 1) | High | Medium | README, docs and CI in Sprint 1 so the work can be picked up by someone else |
| Scheduler state lost on restart (VI-6) | Medium | Low | Keep it in state metadata, not in scheduler fields |

## Backlog after Sprint 3

- Replace the hand-written `hit_block` calls with real instrumentation (SanitizerCoverage `trace-pc-guard` or an LLVM pass). This removes the ID-mapping problem entirely.
- Support multiple targets, and targets given as `file:line`, using DWARF.
- Multi-core fuzzing with the LLMP launcher (the right way to bring back port 1337).
- Migrate to Rust edition 2024 and replace `static mut SIGNALS` with a safe wrapper.
- Test on real targets (for example Magma or a CVE reproduction) once the benchmark method is proven.

## Ceremonies (solo-sized)

- **Monday, 30 min:** planning. Pull the sprint's items into GitHub Issues or a project board.
- **Daily, 2 min:** a one-line note in the PR or issue saying what you did, what's next, and what's blocking.
- **Friday, 45 min:** demo (record the terminal with asciinema) and a retro. Add the results under each sprint heading in this file.
