# OpenDoc Benchmarks

The benchmark runner measures deterministic implemented paths and writes schema
v1 JSON reports. It does not benchmark placeholder capabilities.

## CI Smoke

Run the reduced correctness and report check:

```sh
cargo run -p opendoc-benchmark --release --locked -- \
  --smoke \
  --output target/benchmarks/local-smoke.json
```

Smoke timings from shared machines are not regression gates.

## Full Run

Use a stable environment ID and record the exact source state:

```sh
cargo run -p opendoc-benchmark --release --locked -- \
  --environment-id mac16-12-m4-10c-16gb \
  --source-revision 7581d68 \
  --source-state clean \
  --output target/benchmarks/current.json
```

Compare a compatible report:

```sh
cargo run -p opendoc-benchmark --release --locked -- \
  --environment-id mac16-12-m4-10c-16gb \
  --source-revision <revision> \
  --source-state <clean-or-dirty> \
  --output target/benchmarks/comparison.json \
  --compare benchmarks/baselines/mac16-12-m4-10c-16gb.json
```

Output files must not already exist. This prevents an interrupted or mistaken
run from overwriting reviewed evidence.

## What a comparison does when the sets differ

A baseline older than a workload no longer refuses the whole comparison. Each
case the baseline does not cover is printed as `not covered`, the covered ones
are still gated, and a coverage line says how many of how many. A baseline
holding a case the harness no longer defines is still a hard error: that is a
stale baseline, and comparing against evidence for retired work is worse than
comparing against nothing. `docs/107` §4.2 records why this direction was chosen.

## Run on a quiet machine, and repeat

Medians here move by **3.5x** on one binary and one machine when another build is
running — measured, in `docs/107` §4.2, against `uptime`'s load average. Check the
load first, and repeat a surprising result before believing it. This is also why
the suite holds no timing-ratio gate: a clock-bound check cannot be rescued by a
retry.

## Baseline Review

The current named environment is:

- Apple MacBook Air model identifier `Mac16,12`;
- Apple M4 with 10 logical CPU cores;
- 16 GB memory;
- macOS 26.3, build 25D125;
- Rust 1.96.0 (`ac68faa20`, 2026-05-25).

Do not record serial numbers, hardware UUIDs, usernames, home paths, or other
device identifiers.

Baseline updates follow
`docs/29-BENCHMARK-AND-BASELINE-HARNESS.md`. Generate a new report path, review
the metric and environment differences, then replace the committed baseline as
an intentional source change.
