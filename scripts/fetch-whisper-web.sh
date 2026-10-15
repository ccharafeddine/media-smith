#!/usr/bin/env sh
# Whisper base for the browser build's auto-captions (MIT, OpenAI; ONNX export
# with word timestamps by onnx-community), served same-origin from /models/.
set -e
REV=608c49e61301901684bc36cac8f74b95ff6b5a8e
DIR="$(dirname "$0")/../app/.whisper-web/whisper-base_timestamped"
mkdir -p "$DIR/onnx"
for f in config.json generation_config.json preprocessor_config.json tokenizer.json tokenizer_config.json \
         onnx/encoder_model_quantized.onnx onnx/decoder_model_merged_quantized.onnx; do
  [ -s "$DIR/$f" ] || curl -fL -o "$DIR/$f" "https://huggingface.co/onnx-community/whisper-base_timestamped/resolve/$REV/$f"
done
