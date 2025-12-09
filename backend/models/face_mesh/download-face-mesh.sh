#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODEL_URL="https://s3.ap-northeast-2.wasabisys.com/pinto-model-zoo/282_face_landmark_with_attention/resources.tar.gz"
TARGET_FILE="face_landmark_with_attention.onnx"

echo "Downloading MediaPipe Face Mesh model..."
curl -L "$MODEL_URL" -o resources.tar.gz

echo "Extracting model file..."
# Extract only the float32 ONNX model
tar -zxvf resources.tar.gz face_landmark_with_attention_192x192/model_float32.onnx

echo "Renaming to expected filename..."
mv face_landmark_with_attention_192x192/model_float32.onnx "$SCRIPT_DIR/$TARGET_FILE"

echo "Cleaning up..."
rm -rf resources.tar.gz face_landmark_with_attention_192x192

echo "✓ Download complete: $TARGET_FILE ($(du -h "$SCRIPT_DIR/$TARGET_FILE" | cut -f1))"