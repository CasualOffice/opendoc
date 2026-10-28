#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
#
# Downloads the models the OCR and layout tests are pinned to, verifies their
# SHA-256, and prints the environment variables that switch those tests on.
# TEST-ONLY: the library never fetches a model, and nothing here is committed.
#
#   crates/casual-doc-ocr/fetch-test-models.sh <dir>
#   OPENDOC_OCR_MODELS=<dir> OPENDOC_LAYOUT_MODEL=<dir>/heron.onnx \
#     cargo test --release -p casual-doc-pdf-import --all-features --test ocr
#
# Licences (verified from the model cards, docs/182 §20.1):
#   ocrs detection + recognition  ocrs-models.s3-accelerate.amazonaws.com (what ocrs-cli
#                                 0.13 downloads). No licence file accompanies them; the
#                                 project's only published weights licence is
#                                 CC-BY-SA-4.0 (huggingface.co/robertknight/ocrs).
#   docling-layout-heron (ONNX)   huggingface.co/docling-project/docling-layout-heron-onnx
#                                 Apache-2.0
set -eu

dir=${1:?usage: fetch-test-models.sh <dir>}
mkdir -p "$dir"

fetch() {
  url=$1 out=$2 sum=$3
  if [ ! -f "$dir/$out" ]; then
    curl -fsSL -o "$dir/$out.part" "$url"
    mv "$dir/$out.part" "$dir/$out"
  fi
  got=$(shasum -a 256 "$dir/$out" | cut -d' ' -f1)
  if [ "$got" != "$sum" ]; then
    echo "checksum mismatch for $out: $got" >&2
    exit 1
  fi
}

# The file names keep .rten for the tests; the bytes are ONNX, which rten
# detects and loads.
fetch https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.onnx \
  text-detection.rten a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe
fetch https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.onnx \
  text-recognition.rten 86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5
fetch https://huggingface.co/docling-project/docling-layout-heron-onnx/resolve/main/model.onnx \
  heron.onnx 59c81a3a2923042d85034ffc487f8f47e4854117e879aef89b2b9f728fb4922a

echo "OPENDOC_OCR_MODELS=$dir"
echo "OPENDOC_LAYOUT_MODEL=$dir/heron.onnx"
