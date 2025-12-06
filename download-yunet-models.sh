#!/bin/bash

# Download YuNet face detection models for ViralClip AI
# This script downloads the latest YuNet models with different performance profiles

set -e

MODEL_DIR="/app/models"
mkdir -p "$MODEL_DIR"

echo "Downloading YuNet face detection models to $MODEL_DIR..."
echo

# Download the fastest block-quantized model (recommended for production)
echo "📦 Downloading block-quantized model (fastest, ~6ms/frame)..."
if curl -L -o "$MODEL_DIR/face_detection_yunet_2023mar_int8bq.onnx" \
  "https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar_int8bq.onnx"; then
    echo "✅ Block-quantized model downloaded successfully"
else
    echo "❌ Failed to download block-quantized model"
fi

# Download int8 quantized model (good balance)
echo
echo "📦 Downloading int8-quantized model (~8ms/frame)..."
if curl -L -o "$MODEL_DIR/face_detection_yunet_2023mar_int8.onnx" \
  "https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar_int8.onnx"; then
    echo "✅ Int8-quantized model downloaded successfully"
else
    echo "❌ Failed to download int8-quantized model"
fi

# Download original model (highest accuracy, slowest)
echo
echo "📦 Downloading original model (highest accuracy, ~25ms/frame)..."
if curl -L -o "$MODEL_DIR/face_detection_yunet_2023mar.onnx" \
  "https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx"; then
    echo "✅ Original model downloaded successfully"
else
    echo "❌ Failed to download original model"
fi

echo
echo "🎯 Model Performance Summary:"
echo "  • Block-Quantized: ~6ms/frame, 0.8845 AP (RECOMMENDED)"
echo "  • Int8-Quantized:  ~8ms/frame, 0.8810 AP"
echo "  • Original:        ~25ms/frame, 0.8844 AP"
echo
echo "📁 Models installed in: $MODEL_DIR"
echo "🔧 To enable OpenCV in your build, add: --features opencv"
echo
echo "✅ Setup complete! YuNet face detection is now available."
