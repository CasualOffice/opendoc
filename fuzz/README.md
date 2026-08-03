# OpenDoc Fuzzing

The independent fuzz workspace exercises the format-neutral ZIP, DOCX package,
and ODT package/profile boundaries without adding nightly-only dependencies to
the product workspace. The ODT content target additionally exercises bounded
semantic XML import into the normalized model.

## Prerequisites

- pinned nightly Rust from the CI workflow;
- `cargo-fuzz` 0.13.2;
- a C++11 compiler supported by libFuzzer.

## Build

```sh
cargo +nightly-2026-07-20 fuzz build bounded_package
cargo +nightly-2026-07-20 fuzz build docx_package
cargo +nightly-2026-07-20 fuzz build odt_package
cargo +nightly-2026-07-20 fuzz build odt_content
```

## Seeded Run

Use a disposable corpus because libFuzzer adds discoveries to its first corpus
directory:

```sh
mkdir -p target/fuzz-corpus/docx_package
cp fixtures/generated/*.docx target/fuzz-corpus/docx_package/
cargo +nightly-2026-07-20 fuzz run docx_package \
  target/fuzz-corpus/docx_package -- \
  -max_total_time=60 \
  -timeout=5 \
  -max_len=1048576 \
  -rss_limit_mb=2048
```

Crashes belong under `fuzz/artifacts/` during investigation and are not
committed directly. Minimize and review a reproducer, then add it to the
repository fixture corpus with provenance, checksum, expected outcome, and a
regression test.
