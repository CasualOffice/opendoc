#!/usr/bin/env bash
# Builds the `casual-doc-wasm` bridge into ./pkg for the test viewer.
#
# Requires wasm-pack (https://drager.github.io/wasm-pack/) and the pinned
# toolchain in ../rust-toolchain.toml (Rust 1.96.0 + wasm32-unknown-unknown).
#
# Usage:  ./build.sh          then serve this directory, e.g.
#         python3 -m http.server 8080   →   http://localhost:8080
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/.." && pwd)"

wasm-pack build "$repo/crates/casual-doc-wasm" \
  --target web \
  --out-dir "$here/pkg" \
  --out-name casual_doc_wasm

echo "Built webapp/pkg. Serve this directory and open index.html."
