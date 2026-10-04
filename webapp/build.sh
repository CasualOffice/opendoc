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
  # `web-host-fonts` drops Roboto's four face blobs from the WebAssembly bundle.
  # The browser does not need them: `web_fonts.mjs` fetches the Roboto variable
  # faces from the pinned CDN and registers them before the first paint, so the
  # embedded copy was ~2 MB every visitor downloaded and the engine then replaced.
  # The feature has existed since the font-provisioning work; this build simply
  # never asked for it.
  wasm-pack build "$repo/crates/casual-doc-wasm" \
    --target web \
    --out-dir "$here/pkg" \
    --out-name casual_doc_wasm \
    -- --features web-host-fonts
}

run_wasm_pack_slides() {
  # The PRESENTATION facade, built into the same `pkg` directory under its own
  # `--out-name` so the two bundles sit side by side and neither overwrites the
  # other's glue.
  #
  # Two bundles and not one, deliberately. A visitor who opens a document should
  # not download the deck engine, and a visitor who opens a deck should not
  # download the editor's 16k-line surface — so `editor.html` preloads
  # `casual_doc_wasm` and `slides.html` preloads `casual_pres_wasm`, and the
  # pages share only the chrome vocabulary. Merging them would make every first
  # paint on either page pay for both.
  #
  # Same `web-host-fonts` reasoning as above: the browser fetches the Roboto
  # variable faces from the pinned CDN and registers them before the first paint,
  # so the embedded copy is ~2 MB every visitor downloads and the engine then
  # replaces.
  wasm-pack build "$repo/crates/casual-pres-wasm" \
    --target web \
    --out-dir "$here/pkg" \
    --out-name casual_pres_wasm \
    -- --features web-host-fonts
}

build_with_retry() {
  local attempts=3 i delay
  for ((i = 1; i <= attempts; i++)); do
    # Both facades, and the document one first: it is the larger build and the
    # one whose failure a developer is most likely to be iterating on, so its
    # error appears without waiting for the second.
    if run_wasm_pack && run_wasm_pack_slides; then
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

# The deck the slides page and its browser specs open.
#
# GENERATED here rather than committed, which is the rule `SKILL` §9.1 states and
# the generator's own doc comment repeats: a committed binary fixture drifts from
# the code that claims to produce it, and then the test and the source disagree
# about what is being tested. The builder is the same `src/tests/deck.rs` the
# importer's own guards use, so the browser opens exactly the package those
# guards assert against — three slides named out of lexical order, a hidden
# slide, a theme with a dark colour map, a picture, a group, an unadjusted preset,
# a custom geometry, a 3x3 `a:tbl` with merges on both axes and a style GUID
# joining two `tableStyles.xml` entries, and a chart frame whose payload is
# reported rather than read.
cargo run --quiet -p casual-pres-import --example generate_pptx_fixture -- "$here/demo.pptx"

# --- Static multi-page site -----------------------------------------------
# Inline the shared header/footer partials into every *.page.html template and
# write the flat *.html that GitHub Pages serves. `--check` then fails the build
# if the committed HTML drifted from its template + partials, so the generated
# files can never fall out of sync (the same guard CI runs).
# The embedding guide's code panels, tables and numbers are generated from the
# code they document (webapp/tools/build-embed-docs.mjs). `--check` fails the
# build when the committed page is not what a fresh run produces, so BOTH
# directions of drift are caught here: a page edited by hand, and a source that
# moved without the page being regenerated. Same contract as build-site --check
# below, and as build-embed-package --check for the published package.
node "$here/tools/build-embed-docs.mjs" --check

# The white-label artifacts (docs/126 phase 3, ADR-038): `src/brand.css`,
# `src/brand.mjs` and two generated regions in `editor.html`, all produced from
# `brand.json`. FIRST, because it rewrites `editor.html` and `build-locale`'s
# extraction and `build-site.py`'s templates both read it — and armed here because a
# generator CI never runs is the "prose describing a gate that was never armed"
# defect (docs/99 §9.2). It also re-validates the palette on every build, so a
# deployment cannot drift into unreadable text by editing `src/brand.css` by hand.
node "$here/tools/build-brand.mjs" --check

"$here/build-site.py"
"$here/build-site.py" --check

# The reference pages: the repository's publishable design docs, rendered as real
# pages of this site instead of links to raw Markdown on github.com. Everything on
# them is derived from the committed `.md` — title, description, article body — so
# `--check` fails the build both when a page is hand-edited and when a published
# doc changes without the page being regenerated.
node "$here/tools/build-doc-pages.mjs"
node "$here/tools/build-doc-pages.mjs" --check

# The crawler manifests. `sitemap.xml` and `llms.txt` were hand-written, so a new
# page was invisible to search until somebody remembered two files; they are now
# generated from the pages build-site.py builds, and `--check` fails the build when
# the committed copies drift — which is what makes forgetting impossible rather
# than merely discouraged.
node "$here/tools/build-seo.mjs"
node "$here/tools/build-seo.mjs" --check

echo "Built webapp/pkg, staged the demo/site assets, and generated the static pages."
echo "Run ./serve.py (no-cache), then open http://localhost:8099/."
