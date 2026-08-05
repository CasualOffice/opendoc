#!/usr/bin/env bash
# Builds the `casual-doc-wasm` bridge and stages the static developer site.
#
# Requires wasm-pack (https://drager.github.io/wasm-pack/) and the pinned
# toolchain in ../rust-toolchain.toml (Rust 1.96.0 + wasm32-unknown-unknown).
#
# Usage:  ./build.sh          then serve this directory, e.g.
#         ./serve.py   (no-cache dev server)   →   http://localhost:8099
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/.." && pwd)"

# --- Concurrent-build hardening -------------------------------------------
# Several agents build this webapp at the same time from separate git
# worktrees. Two shared resources make that race, so this script isolates both:
#
#   1. The cargo build-directory lock. Pin the target dir to THIS checkout so
#      concurrent worktree builds never block on each other's build lock
#      ("Blocking waiting for file lock on build directory"). CI has a single
#      checkout, so this stays the usual repo-local ./target that rust-cache
#      already keys on — no CI behaviour change.
#
#   2. The `wasm-opt` binary that wasm-pack fetches on first use into a shared,
#      per-user cache (~/Library/Caches/.wasm-pack or ~/.cache/.wasm-pack).
#      Concurrent first-runs race on that download and fail with "No such file
#      or directory". We serialize only the cold-cache download behind a
#      portable lock; once wasm-opt is cached, builds run fully in parallel.
#      A bounded retry is the final safety net so any lost race self-heals.
export CARGO_TARGET_DIR="$repo/target"

case "$(uname -s)" in
  Darwin) wasm_pack_cache="$HOME/Library/Caches/.wasm-pack" ;;
  *)      wasm_pack_cache="${XDG_CACHE_HOME:-$HOME/.cache}/.wasm-pack" ;;
esac

wasm_opt_cached() {
  compgen -G "$wasm_pack_cache/wasm-opt-*/bin/wasm-opt" >/dev/null 2>&1 \
    || compgen -G "$wasm_pack_cache/wasm-opt-*/wasm-opt" >/dev/null 2>&1
}

run_wasm_pack() {
  wasm-pack build "$repo/crates/casual-doc-wasm" \
    --target web \
    --out-dir "$here/pkg" \
    --out-name casual_doc_wasm
}

build_with_retry() {
  local attempts=3 i delay
  for ((i = 1; i <= attempts; i++)); do
    if run_wasm_pack; then
      return 0
    fi
    if (( i < attempts )); then
      delay=$(( RANDOM % 5 + i ))
      echo "wasm-pack build failed (attempt $i/$attempts); retrying in ${delay}s..." >&2
      sleep "$delay"
    fi
  done
  return 1
}

if wasm_opt_cached; then
  # Warm cache: no shared download to race on — build in parallel.
  build_with_retry
else
  # Cold cache: serialize the one-time wasm-opt download behind a portable
  # lock (mkdir is atomic on POSIX). Peers wait until the cache is warm, then
  # proceed in parallel. The lock is best-effort: after a bounded wait we build
  # anyway and let build_with_retry absorb a lost race.
  lock="${TMPDIR:-/tmp}/opendoc-wasm-opt.lock"
  held=""
  for _ in $(seq 1 600); do
    if mkdir "$lock" 2>/dev/null; then
      held=1
      trap 'rmdir "$lock" 2>/dev/null || true' EXIT
      break
    fi
    wasm_opt_cached && break   # someone else populated it; go build in parallel
    sleep 1
  done
  build_with_retry
  if [[ -n "$held" ]]; then
    rmdir "$lock" 2>/dev/null || true
    trap - EXIT
  fi
fi

mkdir -p "$here/assets"
cp "$repo/fixtures/corpus/real-producer-rich.docx" "$here/demo.docx"
cp "$repo/sample.docx" "$here/sample.docx"
cp "$repo/docs/assets/editor.jpg" "$here/assets/editor.jpg"

# --- Static multi-page site -----------------------------------------------
# Inline the shared header/footer partials into every *.page.html template and
# write the flat *.html that GitHub Pages serves. `--check` then fails the build
# if the committed HTML drifted from its template + partials, so the generated
# files can never fall out of sync (the same guard CI runs).
"$here/build-site.py"
"$here/build-site.py" --check

echo "Built webapp/pkg, staged the demo/site assets, and generated the static pages."
echo "Run ./serve.py (no-cache), then open http://localhost:8099/."
