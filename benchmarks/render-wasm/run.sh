#!/bin/bash
# docs/179 §2 and §5: wasm timings (scalar and simd128 builds) and module sizes.
#
#   benchmarks/render-wasm/run.sh <heavy.docx> [out-dir]
#
# <heavy.docx> is written by
#   cargo run --release -p casual-doc-render --example render_profile -- <heavy.docx>
# Prints CSV to stdout. Size rows are `wasm-opt -Oz` output, the pass
# `webapp/build.sh` runs, and gzip -9 of that.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
heavy="$1"
out="${2:-$here/target}"
mkdir -p "$out"
opt="$(ls "$HOME"/Library/Caches/.wasm-pack/wasm-opt-*/bin/wasm-opt "${XDG_CACHE_HOME:-$HOME/.cache}"/.wasm-pack/wasm-opt-*/bin/wasm-opt 2>/dev/null | head -1 || true)"

build() { # name rustflags features...
  local name=$1 flags=$2; shift 2
  (cd "$here" && CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="$out/$name" RUSTFLAGS="$flags" \
    cargo +1.96.0 build -q --release --target wasm32-unknown-unknown "$@")
  echo "$out/$name/wasm32-unknown-unknown/release/render_wasm_bench.wasm"
}

echo "doc,page,dpi,build,tiny_skia_ms,sparse_ms"
for variant in scalar simd; do
  flags=""
  [ "$variant" = simd ] && flags="-C target-feature=+simd128"
  wasm="$(build "time-$variant" "$flags" --features sparse)"
  node "$here/bench.mjs" "$wasm" "$heavy" "heavy" | sed "s/\$/,$variant/" | awk -F, '{print $1","$2","$3","$6","$4","$5}'
  PAGES=0,1,11,12 node "$here/bench.mjs" "$wasm" "$repo/sample.docx" "sample.docx" | sed "s/\$/,$variant/" | awk -F, '{print $1","$2","$3","$6","$4","$5}'
done

echo
echo "module,raw_bytes,wasm_opt_Oz_bytes,gzip_bytes"
for spec in "base:" "sparse:--features=sparse" "gpu:--features=gpu" "both:--features=sparse,gpu"; do
  name="${spec%%:*}"; feat="${spec#*:}"
  if [ -n "$feat" ]; then wasm="$(build "size-$name" "" "$feat")"; else wasm="$(build "size-$name" "")"; fi
  "$opt" -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
    --enable-mutable-globals "$wasm" -o "$out/$name.opt.wasm"
  echo "$name,$(wc -c <"$wasm" | tr -d ' '),$(wc -c <"$out/$name.opt.wasm" | tr -d ' '),$(gzip -9c "$out/$name.opt.wasm" | wc -c | tr -d ' ')"
done
