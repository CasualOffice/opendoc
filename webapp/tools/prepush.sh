#!/bin/sh
# One command before every push (owner, 2026-11-01): every generator, every
# `--check`, the unit suite, the parity matrix and the golden tests, failing on
# the first stale file or red test. It exists because a generated artefact left
# stale by a docs or command edit (docs/153, the embedding page, a locale) turned
# a PR red after a green local run more than once.
#
# Run from anywhere:   sh webapp/tools/prepush.sh
# It does NOT replace the CI sweep (SKILL §3); it is the set of checks a plain
# `cargo test` sweep misses and that a change to docs/, strings or commands
# can stale. On this machine wrap it in the scratchpad's heavy.sh lock.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"

step() { printf '\n== prepush: %s\n' "$*"; }

step "rustfmt (pinned toolchain)"
cargo +1.96.0 fmt --all --check

step "generators that write committed files"
node webapp/tools/build-glossary.mjs
# en.json (build-locale.mjs) is not run here: it exits non-zero on two
# template-literal keys it cannot resolve, and `locale_artifact.test.mjs` in the
# unit suite below already fails on a stale catalogue.
cargo run --quiet --locked -p opendoc-parity -- write

step "webapp/build.sh (engine, pages, embed docs, SEO, README regions, and their --check)"
./webapp/build.sh

step "parity matrix: check and tests"
cargo run --quiet --locked -p opendoc-parity -- check
cargo test --quiet --locked -p opendoc-parity

step "golden tests"
cargo test --quiet --locked -p casual-doc-layout --test geometry_snapshot
cargo test --quiet --locked -p opendoc --test deterministic_pixels
cargo test --quiet --locked -p casual-doc-transaction --lib codec

step "unit suite (ratchets, locale coverage, tracker guards)"
(cd webapp && npm run test:unit)

step "nothing generated is left uncommitted"
if ! git diff --quiet -- . ':!webapp/pkg' ':!webapp/pkg-pdf'; then
  echo "prepush: the generators changed committed files; review and commit them:" >&2
  git status --short >&2
  exit 1
fi
echo
echo "prepush: all generators current, all checks green."
