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
