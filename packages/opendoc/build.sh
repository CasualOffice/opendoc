#!/usr/bin/env bash
# Builds @casualoffice/opendoc (the headless JS/TS SDK, docs/176 section 12)
# from crates/opendoc with the `wasm` feature.
#
#   ./packages/opendoc/build.sh
#
# Output, all under pkg/ (wasm-pack writes a `*` .gitignore there, so nothing
# here is ever committed): pkg/opendoc_bg.wasm and its glue pkg/opendoc.js.
#
# Fonts. Unlike the Viewer package, this module embeds the WHOLE bundled face
# set (Liberation, Carlito, Caladea, Roboto, Noto Emoji, Noto Sans Symbols 2): a headless SDK runs
# on a server or in a build step with no host to serve faces, and the pixels of
# a render must not depend on what a machine has installed. The cost is the
# download, which is stated in the README and measured by tools/measure-size.mjs.
#
# The optimiser settings are webapp/build.sh's: release opt-level z with the
# root Cargo.toml's per-package opt-level 3 overrides, and `wasm-opt -Oz`.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}"
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_RELEASE_OPT_LEVEL=z

wasm-pack build "$repo/crates/opendoc" \
  --release --target web \
  --out-dir "$here/pkg" --out-name opendoc \
  --no-pack \
  -- --features wasm

# The embedded faces' licences travel with the module (NOTICE section 1).
# wasm-pack writes pkg/.gitignore = "*", which npm also honours inside pkg/ and
# which would drop the licences below from the tarball. The package's own
# .gitignore keeps pkg/ out of git.
rm -f "$here/pkg/.gitignore"
mkdir -p "$here/pkg/LICENSES"
cp "$repo/crates/casual-doc-layout/fonts/LICENSES/"*.txt "$here/pkg/LICENSES/"

echo "built $here/pkg"
