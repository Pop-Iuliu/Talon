# Evaluation: does directed scheduling help?

Question: does Talon's distance-guided scheduler reach the crash faster than
plain coverage-guided baselines?

## Method

- Target: `Tests/if_nest_hard.c`. The crash sits behind ten sequential byte
  checks (`TALON\x01\xDE\xAD\xBE\xEF`, one block each), plus two decoy
  branches full of coverage (lowercase and digit first bytes) that lead away
  from the bug.
- Schedulers: `directed` (annealed selection + power stage), `queue` (FIFO
  round-robin), `rand` (uniform random). All three share the same power stage;
  without distance metadata it hands out a fixed mutation budget, so the
  baselines are not handicapped.
- 10 trials per scheduler with paired seeds (`--seed 1..10`), 60 s budget,
  15 s cooling, release build, 4096-entry signal map.
- Metric: time to exposure (TTE), the wall-clock time until the crash is
  saved. A trial that times out is recorded at 60 s and counted as not found.
- Statistics: median TTE, Vargha-Delaney A12 (probability that a directed
  trial is faster, plus half the ties), and a two-sided Mann-Whitney U test
  with a normal approximation (`tools/analyze_bench.py`).

Reproduce with `./bench.sh`.

## Results

| Scheduler | Found | Median TTE (s) | A12 vs directed | Mann-Whitney p |
|---|---|---|---|---|
| directed | 9/10 | 10.89 | - | - |
| queue | 10/10 | 9.43 | 0.350 | 0.2568 |
| rand | 10/10 | 11.98 | 0.410 | 0.4963 |

## Interpretation

Negative result: on this target, the directed scheduler does not beat the
baselines. The medians are within noise (p = 0.26 and 0.50) and the point
estimates slightly favour the queue scheduler.

Likely reasons, in decreasing order of confidence:

1. **Selection overhead.** The weighted pick walks the corpus and rebuilds a
   cumulative weight vector on every selection. Measured throughput is
   roughly 280-310k exec/s for `directed` versus about 530k for `queue`, so
   the directed scheduler pays about a factor of two per execution and buys
   nothing on a target this small.
2. **Coverage guidance already solves this shape.** Each magic byte unlocks a
   new block, so classic max-map feedback has a gradient for every step. The
   directed scheduler can only re-weight seeds the baselines would reach
   anyway.
3. **Short campaigns.** At 60 s the annealing spends 15 s exploring and the
   differences between seeds are small; cooling effects would need longer
   campaigns to matter.

The target was deliberately built with a coverage gradient at every magic
byte. Joint two-byte checks (`DE AD` as a single condition) were tried first:
they create a feedback barrier that no scheduler can guide through, and the
campaign degenerates into uniform search at roughly 1/65536 per barrier.

## What would sharpen this

- Cache the cumulative weights and rebuild them only when the corpus changes,
  removing the per-selection overhead.
- Longer campaigns (minutes, not seconds) where the cooling schedule and
  seed distances have time to differentiate.
- Targets where coverage guidance stalls: joint constraints without
  intermediate feedback, or targets deeper than a dozen blocks.
- Multi-core fuzzing so the comparison is not dominated by a 64 KiB map
  reset per execution.
