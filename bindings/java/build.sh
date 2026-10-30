#!/usr/bin/env bash
# Builds and tests the Java binding (docs/176 section 12.9) locally:
#
#   ./bindings/java/build.sh
#
# 1. the native library: crates/opendoc-ffi (cdylib), release;
# 2. the classes and the jar (JDK 22+: the Foreign Function & Memory API);
# 3. the conformance test against the committed sample.docx.
#
# Output under bindings/java/build/ (git-ignored). Nothing is published; a
# Maven Central release is the owner's separate decision.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}"

cargo build -p opendoc-ffi --release --locked

case "$(uname -s)" in
  Darwin) lib="libopendoc_ffi.dylib" ;;
  MINGW*|MSYS*|CYGWIN*) lib="opendoc_ffi.dll" ;;
  *) lib="libopendoc_ffi.so" ;;
esac
native="$CARGO_TARGET_DIR/release/$lib"

rm -rf "$here/build"
mkdir -p "$here/build/classes" "$here/build/test-classes"
javac --release 22 -Xlint:all,-restricted -Werror -d "$here/build/classes" \
  $(find "$here/src/main/java" -name '*.java')
javac --release 22 -d "$here/build/test-classes" -cp "$here/build/classes" \
  $(find "$here/src/test/java" -name '*.java')
jar --create --file "$here/build/opendoc.jar" -C "$here/build/classes" .

java --enable-native-access=ALL-UNNAMED -Dopendoc.library="$native" \
  -cp "$here/build/classes:$here/build/test-classes" \
  org.casualoffice.opendoc.ConformanceTest "$repo/sample.docx"
echo "built $here/build/opendoc.jar against $native"
