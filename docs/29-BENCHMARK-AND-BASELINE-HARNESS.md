# Benchmark and Baseline Harness

**Status:** Accepted for Phase 0
**Decision date:** 2026-07-24
**Tracker:** P0-007
**Implementation:** Pending

## Outcome

Add a repository-owned benchmark runner that measures implemented package and
model paths, emits a versioned machine-readable report, and can compare a
current report with a baseline from the same named environment.

The Phase 0 harness establishes measurement discipline. It does not claim the
load, pagination, typing, save, or memory targets for capabilities that have not
been implemented.

## Boundary

The runner lives in `tools/opendoc-benchmark` as a non-published workspace
binary. It may depend on internal OpenDoc crates, but product crates do not
depend on it.

The runner uses the standard library for timing and the workspace's existing
serialization dependencies for reports. A third-party statistics framework is
not needed for the initial fixed workloads and would add compile time and
dependency surface without improving the Phase 0 decisions.

## Initial Workloads

Every workload has a stable ID, fixed work definition, deterministic input, and
an output checksum that is validated outside the timed region.

| ID | Work definition | Evidence |
| --- | --- | --- |
| `docx.package_open.minimal` | Admit the generated minimal DOCX with default limits. | ZIP preflight and metadata construction. |
| `docx.part_read.document_xml` | Open the minimal package and read `word/document.xml`. | Bounded Deflate read and CRC verification. |
| `model.normalized_load.100_paragraphs` | Open generated schema v0 JSON containing 100 text paragraphs. | Bounded parse, model validation, and session construction. |
| `sdk.typing.100_graphemes` | Create a blank session and apply 100 single-grapheme insert transactions. | Public transaction, selection mapping, history, and event paths. |

Fixture bytes are embedded or generated before sampling so filesystem latency is
not accidentally included. Workload changes require a new ID or an explicit
schema-compatible revision recorded in both design and baseline review.

## Sampling

The runner uses a monotonic clock and `std::hint::black_box`.

- warm-up samples are executed but never reported;
- each measured sample executes a fixed number of workload iterations;
- elapsed nanoseconds are divided by completed iterations;
- samples are sorted before computing minimum, median, and nearest-rank p95;
- invalid output, zero elapsed time, arithmetic overflow, or an incomplete
  iteration fails the run;
- full mode uses enough samples and iterations to reduce timer granularity;
- smoke mode uses reduced counts and validates behavior and report generation,
  not performance.

The exact warm-up, sample, and iteration counts are written into every report.
Changing them is a reviewable benchmark-definition change.

## Report Schema

JSON report schema version 1 contains:

- runner version and source revision;
- UTC Unix timestamp;
- build profile;
- explicit environment ID;
- operating system, architecture, Rust version, and logical CPU count;
- smoke/full mode;
- workload ID, work units, sample configuration, and output checksum;
- minimum, median, and p95 nanoseconds per iteration.

Fields use deterministic ordering through typed serialization. Reports never
include usernames, home paths, document content, or unrestricted environment
variables.

Reports are written atomically by creating a sibling temporary file and
renaming it only after complete serialization.

## Environment Identity

A baseline is meaningful only on a controlled environment. Full runs require an
explicit `--environment-id`; the ID describes a maintained runner class, not a
person or transient CI job.

Comparison requires matching:

- schema version;
- non-smoke mode;
- release build profile;
- environment ID;
- operating system and architecture;
- workload IDs and work units;
- iterations per sample.

Rust patch versions are reported but do not prevent comparison. Toolchain
changes remain visible review inputs because compiler changes can legitimately
move a baseline.

## Regression Policy

The comparison metric is median nanoseconds per iteration. Each baseline case
records:

- maximum relative regression in basis points;
- absolute noise allowance in nanoseconds.

The current result passes when it is no slower than the baseline median plus the
larger of the relative or absolute allowance. Missing, duplicate, or unexpected
cases fail comparison.

Phase 0 starts with a 20% relative allowance and a case-specific absolute noise
floor. These thresholds are deliberately conservative until a dedicated runner
has enough history to quantify variance.

Shared GitHub-hosted runner timings do not block pull requests. Pull-request CI
runs release-mode smoke workloads and validates the report. Baseline comparison
becomes blocking only on a named, controlled runner through a separate workflow
decision.

## Baseline Storage

Reviewed reports live under:

```text
benchmarks/
├── README.md
├── baselines/
│   └── <environment-id>.json
└── reports/
    └── phase0-status.md
```

`target/benchmarks/` contains uncommitted local and CI output.

A baseline update requires:

- the exact generation command;
- source revision and clean/dirty state;
- environment identity;
- before/after metric summary;
- explanation for regressions;
- profiler evidence for unexplained hot-path regressions;
- reviewer acknowledgement.

Baselines are not regenerated merely to make a comparison pass.

## CLI Contract

The native runner supports:

```text
opendoc-benchmark --smoke --output <path>
opendoc-benchmark --environment-id <id> --source-revision <sha> --output <path>
opendoc-benchmark ... --compare <baseline> [--max-regression-percent <value>]
```

`--max-regression-percent` may lower a baseline allowance for investigation but
cannot relax the committed threshold. Unknown options, duplicate options,
missing values, non-finite percentages, and unsafe output paths fail with a
concise non-zero exit.

The tool compiles to `wasm32-unknown-unknown` as an inert target so the workspace
portability gate remains complete. Benchmark execution is native-only.

## CI Contract

Add a required `benchmark-smoke` job that:

1. builds the runner in release mode;
2. executes the reduced deterministic workloads;
3. writes a report under `target/benchmarks/`;
4. validates schema version, smoke mode, and expected workload IDs.

This job proves that performance-sensitive entry points and reporting stay
executable. It does not enforce wall-clock thresholds on shared infrastructure.

## Failure Behavior

The runner returns a non-zero exit for:

- invalid CLI configuration;
- workload setup or correctness failure;
- report serialization or atomic-write failure;
- incompatible baseline metadata;
- missing, duplicate, or unexpected workload cases;
- a threshold regression.

Error output names the workload or metadata field but does not include document
content.

## Rejected Alternatives

### Gate pull requests on hosted-runner timing

Rejected because host contention and VM variation create false regressions and
encourage threshold inflation.

### Commit only human-readable benchmark prose

Rejected because comparisons, trend tooling, and auditability need a versioned
machine-readable source.

### Introduce Criterion immediately

Rejected for the initial fixed workload set. The custom runner needs explicit
environment matching, checksums, report policy, and cross-platform smoke
behavior regardless; a statistics dependency can be reconsidered when the
benchmark matrix becomes substantially larger.

### Benchmark unimplemented layout and rendering paths

Rejected because placeholder numbers would create false progress. The schema
can add those workload IDs when the underlying capabilities exist.

## Acceptance Gates

- design and CLI/report contracts are documented before implementation;
- all four initial workloads validate deterministic outputs;
- report schema and percentile/threshold math have unit tests;
- reports are atomically written and free of local paths or document content;
- full-mode baseline comparison rejects incompatible environments and
  regressions;
- release-mode benchmark smoke runs in pull-request CI;
- one reviewed Phase 0 baseline and status report are committed;
- native, Windows, macOS, WASM, MSRV, docs, lint, audit, and policy gates pass.
