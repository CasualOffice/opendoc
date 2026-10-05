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

# --- Size, for the browser only -------------------------------------------
# `wasm-pack build` already builds the `release` profile (it prints "Finished
# `release` profile [optimized]"), and the workspace profile sets
# `codegen-units = 1`, `lto = "thin"`, `strip = "symbols"` — but no `opt-level`,
# so it defaults to 3: optimise for SPEED. For a native binary that is right.
# For a 26 MB WebAssembly module it is not: the browser must download AND
# compile every byte of the code section before the editor is interactive, and
# `opt-level = 3`'s inlining and loop unrolling buy throughput by emitting more
# code.
#
# This `z` is only half the setting. The other half is the per-package
# `opt-level = 3` overrides in the root `Cargo.toml`, which keep the layout,
# shaping, rasterization, model and import crates at 3 so only the cold bulk is
# built for size — because `z` applied to EVERYTHING costs 2.5x on a page render
# and 1.7x on a document open, which is not a trade this editor can make. The
# measured table is in that `Cargo.toml` block; read the two together.
#
# Measured on this tree (final post-`wasm-opt` artifact, `web-host-fonts` on):
#
#   rust opt-level 3, wasm-opt -O     26,070,655 B   code 12,376,042   baseline
#   rust opt-level 3, wasm-opt -Oz    25,800,405 B                      -1.0%
#   rust opt-level s, wasm-opt -Oz    22,130,417 B   code  8,492,515   -15.1%
#   this build (z + the 3 overrides)  24,297,780 B   code 10,594,266    -6.8%
#   rust opt-level z, wasm-opt -Oz    20,906,038 B   code  7,232,706   -19.8%
#
# so the code section the browser must COMPILE falls 14.4% and the download
# 1,772,875 B. Be careful quoting the compressed figure: gzip only falls
# 11,521,748 -> 11,332,567 B (-1.6%), because what is left is the DATA section
# and half of that is font bytes, which barely compress and which no
# optimisation level touches at all — see the note at the end of this block.
# If the module is served compressed, this setting is worth ~190 KB a load; if
# it is served raw, 1.77 MB.
#
# This is set HERE rather than in `[profile.release]` on purpose. The same
# profile builds the native engine, the `opendoc-benchmark` smoke gate
# (`cargo run -p opendoc-benchmark --release`, which asserts
# `.buildProfile == "release"`) and the headless render tools, and those want
# speed, not size. A `[profile.release] opt-level = "z"` would silently slow
# every native consumer to shrink a browser download. `build.sh` is the single
# entry point both CI jobs that produce `webapp/pkg` use (ci.yml browser-smoke
# and pages.yml), so scoping the override to this script keeps the trade where
# the trade applies. `CARGO_PROFILE_<name>_<key>` is cargo's documented
# per-invocation profile override, so this needs no custom profile and no
# wasm-pack version that supports `--profile`.
#
# Rejected after measuring, so nobody re-tries them:
#   * `opt-level = z` or `s` applied to EVERY crate — 20,906,038 B and
#                          22,130,417 B, but 2.49x and 2.02x on a page render.
#                          The whole reason the per-package overrides exist.
#   * `lto = "fat"`      — 20,893,675 B, 10,020 B (0.05%) smaller than thin, for
#                          a build that went from 56 s to 7 m 32 s.
#   * `panic = "abort"`  — 20,902,646 B, 1,049 B smaller. `wasm32-unknown-unknown`
#                          already aborts; there are no unwind tables to drop.
#                          (Nothing on the wasm path calls `catch_unwind`; the
#                          only caller is the native `opendoc-render` tool, which
#                          is why this must never become a profile-wide setting.)
#   * a dedicated `[profile.wasm-size]` with `--profile` — wasm-pack only reads
#                          `[package.metadata.wasm-pack.profile.release]`'s
#                          wasm-opt flags for its three named profiles, and CI
#                          pins a different wasm-pack version (0.15.0) than is
#                          installed here, so a custom profile could not be
#                          verified locally against the version that deploys.
#                          `CARGO_PROFILE_<name>_<key>` needs neither.
export CARGO_PROFILE_RELEASE_OPT_LEVEL=z

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
  #
  # It only drops ROBOTO. Measured on the artifact this script produces, the
  # module's 13.6 MB data section still carries 9,317,088 B of font faces that
  # no `opt-level` can touch, because they are `include_bytes!` asset bytes in
  # `casual-doc-layout/src/fonts.rs` with no `cfg` on them:
  #
  #   Liberation Sans/Serif/Mono (12 faces)  4,359,164 B
  #   Carlito (4 faces)                      2,733,784 B
  #   NotoEmoji-Variable                     1,982,596 B
  #   Caladea (4 faces)                        241,544 B
  #
  # That is 36% of the whole download and the single largest item in it — larger
  # than the entire code section after this script's size settings. Extending
  # `web-host-fonts` to them is NOT a build-script change: each is a
  # metric-compatible substitute the engine relies on offline (Carlito for
  # Calibri, Liberation for Arial/Times/Courier, Noto Emoji so an emoji is never
  # tofu), so dropping the bytes requires the host to register equivalents first
  # — `webapp/src/web_fonts.mjs` currently fetches Roboto, Noto Sans and Noto
  # Serif and none of these four families. Owner call, font-provisioning lane.
  wasm-pack build "$repo/crates/casual-doc-wasm" \
    --target web \
    --out-dir "$here/pkg" \
    --out-name casual_doc_wasm \
    -- --features web-host-fonts
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
