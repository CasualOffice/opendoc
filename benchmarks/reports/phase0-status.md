# Phase 0 Benchmark Status

**Captured:** 2026-07-24
**Source revision:** `7581d68` (clean)
**Environment:** `mac16-12-m4-10c-16gb`
**Report schema:** 1
**Build profile:** release

## Results

| Workload | Work | Median | p95 |
| --- | ---: | ---: | ---: |
| DOCX minimal package admission | 3 parts | 3.044 us | 4.688 us |
| DOCX `document.xml` read | 34 bytes | 3.666 us | 4.674 us |
| Normalized JSON load | 100 paragraphs | 49.594 us | 49.876 us |
| SDK typing transaction path | 100 graphemes | 313.750 us | 318.675 us |

The committed source report is
`benchmarks/baselines/mac16-12-m4-10c-16gb.json`. A second full run on the same
environment passed its threshold comparison before this status report was
committed.

## Interpretation

These numbers establish the first reproducible package/model baseline. They are
not the user-facing performance targets from `docs/01-ORD.md`: semantic DOCX
load, first page, pagination, rendering, save, and memory measurement do not
exist yet and therefore have no benchmark claim.

GitHub pull requests run the same workload entry points in smoke mode. Timing
regression comparison remains manual on this named environment until a
controlled dedicated runner is provisioned.
