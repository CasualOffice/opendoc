#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
#
# Downloads the models the OCR and layout tests are pinned to, verifies the
# SHA-256 of every downloaded file, converts PP-OCRv5 to ONNX, and prints the
# environment variables that switch the model tests on. TEST-ONLY: the library
# never fetches or converts a model, and nothing here is committed.
#
#   crates/casual-doc-ocr/fetch-test-models.sh <dir>
#   OPENDOC_PADDLE_MODELS=<dir>/paddle OPENDOC_OCR_MODELS=<dir>/ocrs \
#   OPENDOC_LAYOUT_MODEL=<dir>/heron.onnx \
#     cargo test --release -p casual-doc-pdf-import --all-features --test ocr
#
# PP-OCRv5 needs a Python with PaddlePaddle's converter (`pip install
# paddle2onnx paddlepaddle pyyaml`, measured with paddle2onnx 2.1.0 and
# paddlepaddle 3.3.1); set PYTHON to it. Without one the PP-OCRv5 step is
# skipped and its tests print why.
#
# Licences (primary sources, docs/182 §20.1):
#   PP-OCRv5 mobile det/rec, latin rec  huggingface.co/PaddlePaddle/*  Apache-2.0 (model cards)
#   PP-LCNet_x1_0_textline_ori          huggingface.co/PaddlePaddle/PP-LCNet_x1_0_textline_ori
#                                       Apache-2.0 (model card), the text-line orientation
#                                       classifier (docs/182 §20a)
#   ocrs detection + recognition        ocrs-models.s3-accelerate.amazonaws.com (what
#                                       ocrs-cli 0.13 downloads); no licence file with them,
#                                       the project's only weights licence is CC-BY-SA-4.0
#                                       (huggingface.co/robertknight/ocrs)
#   docling-layout-heron (ONNX)         huggingface.co/docling-project/docling-layout-heron-onnx
#                                       Apache-2.0
set -eu

dir=${1:?usage: fetch-test-models.sh <dir>}
mkdir -p "$dir/paddle/det" "$dir/paddle/rec" "$dir/paddle/latin_rec" "$dir/paddle/textline_ori" "$dir/ocrs"

fetch() {
  url=$1 out=$2 sum=$3
  if [ ! -f "$out" ]; then
    curl -fsSL -o "$out.part" "$url"
    mv "$out.part" "$out"
  fi
  got=$(shasum -a 256 "$out" | cut -d' ' -f1)
  if [ "$got" != "$sum" ]; then
    echo "checksum mismatch for $out: $got" >&2
    exit 1
  fi
}

hf=https://huggingface.co/PaddlePaddle
for pair in "PP-OCRv5_mobile_det det afa1820cb16c1fd0dad589d0f8b389139061c1ef6d68019685fd07be997dda5b" \
            "PP-OCRv5_mobile_rec rec 2460da90875937c94db97eba74ae3d9e5d4c4c57c42f1f41531c09a26bcc771a" \
            "latin_PP-OCRv5_mobile_rec latin_rec 53cdc8b481a7394bb108f96d0fb3432b0a8f392e22c7d18f06dbb2d42b8b25f9" \
            "PP-LCNet_x1_0_textline_ori textline_ori 0de2bcf996cf553e2b848dd7b1769dafffc6917b1ccdf55c1d8efe7909fbf743"; do
  set -- $pair
  repo=$1 name=$2 params=$3
  # The weights are pinned; the graph and config files come from the same
  # repository revision and are read only by the converter.
  fetch "$hf/$repo/resolve/main/inference.pdiparams" "$dir/paddle/$name/inference.pdiparams" "$params"
  for f in inference.json inference.yml; do
    [ -f "$dir/paddle/$name/$f" ] || curl -fsSL -o "$dir/paddle/$name/$f" "$hf/$repo/resolve/main/$f"
  done
done

if ${PYTHON:-python3} -c 'import paddle2onnx, yaml' 2>/dev/null; then
  for name in det rec latin_rec textline_ori; do
    [ -f "$dir/paddle/$name.onnx" ] || "$(dirname "$(command -v "${PYTHON:-python3}")")/paddle2onnx" \
      --model_dir "$dir/paddle/$name" --model_filename inference.json \
      --params_filename inference.pdiparams --save_file "$dir/paddle/$name.onnx" \
      --opset_version 14
  done
  ${PYTHON:-python3} - "$dir/paddle" <<'PY'
import sys, yaml
d = sys.argv[1]
for m in ["rec", "latin_rec"]:
    # BaseLoader: every entry stays a string (safe_load would turn some into
    # numbers or booleans).
    cfg = yaml.load(open(f"{d}/{m}/inference.yml", encoding="utf-8"), Loader=yaml.BaseLoader)
    with open(f"{d}/{m}.dict.txt", "w", encoding="utf-8") as f:
        for c in cfg["PostProcess"]["character_dict"]:
            f.write(c + "\n")
PY
else
  echo "PP-OCRv5 not converted: set PYTHON to a Python with paddle2onnx, paddlepaddle and pyyaml" >&2
fi

# The file names keep .rten for the tests; the bytes are ONNX, which rten
# detects and loads.
fetch https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.onnx \
  "$dir/ocrs/text-detection.rten" a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe
fetch https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.onnx \
  "$dir/ocrs/text-recognition.rten" 86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5
fetch https://huggingface.co/docling-project/docling-layout-heron-onnx/resolve/main/model.onnx \
  "$dir/heron.onnx" 59c81a3a2923042d85034ffc487f8f47e4854117e879aef89b2b9f728fb4922a

# The directory as a whole is what `opendoc-mcp --ocr-models` and
# `OcrModels::from_dir` read (paddle/det.onnx, paddle/<pack>.onnx with
# paddle/<pack>.dict.txt, paddle/textline_ori.onnx, heron.onnx).
echo "OPENDOC_PADDLE_MODELS=$dir/paddle"
echo "OPENDOC_OCR_MODELS=$dir/ocrs"
echo "OPENDOC_LAYOUT_MODEL=$dir/heron.onnx"
