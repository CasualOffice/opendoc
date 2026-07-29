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

wasm-pack build "$repo/crates/casual-doc-wasm" \
  --target web \
  --out-dir "$here/pkg" \
  --out-name casual_doc_wasm

mkdir -p "$here/assets"
cp "$repo/fixtures/corpus/real-producer-rich.docx" "$here/demo.docx"
cp "$repo/sample.docx" "$here/sample.docx"
cp "$repo/docs/assets/editor.jpg" "$here/assets/editor.jpg"

echo "Built webapp/pkg and staged the demo/site assets."
echo "Run ./serve.py (no-cache), then open http://localhost:8099/."
