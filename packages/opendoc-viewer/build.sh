#!/usr/bin/env bash
# Builds @casualoffice/opendoc-viewer from crates/opendoc-viewer (docs/176).
#
#   ./packages/opendoc-viewer/build.sh            # the package as shipped
#   VIEWER_FEATURES=wasm,core-fonts,pdf,formats ./packages/opendoc-viewer/build.sh
#
# Output, all under pkg/ (wasm-pack writes a `*` .gitignore there, so nothing
# here is ever committed):
#
#   pkg/opendoc_viewer_bg.wasm, pkg/opendoc_viewer.js   the engine and its glue
#   pkg/fonts/                                          the host-served faces
#
# What ships, and why (the owner's decisions, docs/176 section 10):
#
#   * DOCX only. ODT, RTF and TXT are the opt-in `formats` feature (+3.6 MB raw
#     until prerequisite P1), so a host that wants them builds with
#     VIEWER_FEATURES.
#   * PDF on. It is 160 KB gzip, and a viewer that cannot print is not one.
#   * A small core font set INSIDE the module (`core-fonts`: Liberation Sans),
#     and every other face as a plain file in pkg/fonts/ that the HOST serves
#     and hands back through the font hook. Nothing is fetched from a CDN.
#
# The optimiser settings are webapp/build.sh's, so the editor and the viewer are
# measured the same way: release opt-level z with the root Cargo.toml's
# per-package opt-level 3 overrides, and the crate's own `wasm-opt -Oz`.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}"
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_RELEASE_OPT_LEVEL=z

features="${VIEWER_FEATURES:-wasm,core-fonts,pdf}"

wasm-pack build "$repo/crates/opendoc-viewer" \
  --release --target web \
  --out-dir "$here/pkg" --out-name opendoc_viewer \
  --no-pack \
  -- --features "$features"

# The faces the module does NOT embed, for the host to serve. Liberation Sans is
# in the module already, so it is not copied twice.
fonts="$repo/crates/casual-doc-layout/fonts"
out="$here/pkg/fonts"
rm -rf "$out"
mkdir -p "$out/LICENSES"
for face in \
  Carlito-Regular Carlito-Bold Carlito-Italic Carlito-BoldItalic \
  Caladea-Regular Caladea-Bold Caladea-Italic Caladea-BoldItalic \
  Roboto-Regular Roboto-Bold Roboto-Italic Roboto-BoldItalic \
  NotoEmoji-Variable NotoSansSymbols2-Subset; do
  cp "$fonts/$face.ttf" "$out/"
done
for face in \
  LiberationSerif-Regular LiberationSerif-Bold LiberationSerif-Italic LiberationSerif-BoldItalic \
  LiberationMono-Regular LiberationMono-Bold LiberationMono-Italic LiberationMono-BoldItalic; do
  cp "$fonts/liberation/$face.ttf" "$out/"
done
cp "$fonts/LICENSES/"*.txt "$out/LICENSES/"
cp "$fonts/README.md" "$out/README.md"

echo "built $here/pkg with features: $features"
