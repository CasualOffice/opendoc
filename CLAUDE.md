# Claude Code Instructions

**Load the `opendoc` skill first** (`.claude/skills/opendoc/SKILL.md`), before touching
anything in this repository. It carries the product goal, the non-negotiable gates —
including the two CI gates a normal `cargo test` sweep misses — the PR and branching rules,
how to parallelise with agents, the known-flaky tests, and the specific mistakes already
made here. It exists so the owner does not have to repeat the same instructions every
session; keep it current when a rule changes.

Then follow [AGENTS.md](AGENTS.md).

This repository uses the same working contract for Claude Code, Codex, and other coding agents:

- this repo is the implementation target;
- the sibling `docs` repo is reference-only unless explicitly requested;
- design first, discuss, finalize, track, then implement;
- production-grade quality is the baseline.
