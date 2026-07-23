# Phase 0 DOCX Package Fuzz Report

**Date:** 2026-07-24
**Source revision:** `0f1a746` (clean)
**Target:** `docx_package`
**Result:** Pass

## Environment

- architecture: Apple arm64;
- nightly: `nightly-2026-07-20`, rustc `1.99.0-nightly`
  (`9f36de775`, 2026-07-19);
- cargo-fuzz: 0.13.2;
- libfuzzer-sys: 0.4.13;
- production compression graph: zip 7.0.0, flate2 1.1.9, zlib-rs 0.6.3.

No usernames, machine serials, hardware UUIDs, home paths, or input content are
recorded in this report.

## Seeds

The campaign started from all seven generated fixtures:

- minimal valid;
- mixed Unicode;
- unknown safe part;
- path traversal;
- high expansion;
- duplicate part;
- malformed truncated package.

Their source, license, expected outcomes, and SHA-256 values are recorded in
`fixtures/manifest.json`.

## Limits

- input: 1 MiB;
- entries: 128;
- total expanded bytes: 8 MiB;
- single expanded part: 2 MiB;
- expansion ratio: 100:1;
- path bytes: 512;
- per-input timeout: 5 seconds;
- process RSS: 2 GiB.

## Campaigns

### Time-bounded

The clean revision completed a 15-second seeded campaign with exit code 0. No
crash, panic, timeout, sanitizer finding, or artifact was produced.

### Counted

The clean revision completed exactly 100,000 seeded executions with exit code
0. No crash, panic, timeout, sanitizer finding, or artifact was produced.

## Interpretation

This is a Phase 0 smoke result, not a claim that fuzzing is exhaustive. Pull
requests compile the target, and the scheduled security workflow runs a
60-second campaign. Future XML, relationship, image, and semantic parsers
require their own structure-aware targets.
