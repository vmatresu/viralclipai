#!/bin/bash

# =============================================================================
# YuNet Face Detection Model Downloader
# =============================================================================
# Downloads and validates YuNet face detection models for ViralClip AI
#
# Models are saved to: backend/models/face_detection/yunet/
# This allows them to be committed to version control for reproducible builds
#
# Usage:
#   ./download-yunet-models.sh        # Download all models
#   ./download-yunet-models.sh --verify # Verify existing models only
# =============================================================================

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR" && pwd)"
MODEL_DIR="$PROJECT_ROOT/backend/models/face_detection/yunet"
BACKUP_DIR="$MODEL_DIR/backup"

# Expected model information
MODEL_INFO=(
    "face_detection_yunet_2023mar.onnx|https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx|230000|float32|25ms"
    "face_detection_yunet_2023mar_int8.onnx|https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar_int8.onnx|100000|int8|8ms"
    "face_detection_yunet_2023mar_int8bq.onnx|https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar_int8bq.onnx|120000|int8bq|6ms"
)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() { echo -e "${BLUE}[INFO]${NC} $1" >&2; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1" >&2; }
log_error() { echo -e "${RED}[ERROR]${NC} $1" >&2; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1" >&2; }

# Check if running in verify-only mode
VERIFY_ONLY=false
if [[ "${1:-}" == "--verify" ]]; then
    VERIFY_ONLY=true
fi

# Create directories
create_directories() {
    log_info "Creating model directories..."
    mkdir -p "$MODEL_DIR"
    mkdir -p "$BACKUP_DIR"

    # Create .gitkeep to ensure directory is tracked
    touch "$MODEL_DIR/.gitkeep"
}

# Verify model integrity
verify_model() {
    local model_name="$1"
    local expected_size="$2"
    local model_path="$MODEL_DIR/$model_name"

    if [[ ! -f "$model_path" ]]; then
        log_error "Model file missing: $model_path"
        return 1
    fi

    local actual_size
    actual_size=$(stat -f%z "$model_path" 2>/dev/null || stat -c%s "$model_path" 2>/dev/null || echo "0")

    if [[ "$actual_size" -eq 0 ]]; then
        log_error "Model file is empty: $model_path"
        return 1
    fi

    # Allow 20% tolerance for file size (models can vary slightly between versions)
    local min_size=$((expected_size * 8 / 10))
    local max_size=$((expected_size * 12 / 10))

    if [[ "$actual_size" -lt "$min_size" || "$actual_size" -gt "$max_size" ]]; then
        log_warn "Model size mismatch: expected ~${expected_size}B, got ${actual_size}B for $model_name"
        return 1
    fi

    log_success "✓ $model_name (${actual_size}B) - valid"
    return 0
}

# Download single model with retry logic
download_model() {
    local model_name="$1"
    local model_url="$2"
    local expected_size="$3"
    local model_path="$MODEL_DIR/$model_name"

    log_info "Downloading $model_name..."

    # Backup existing file if it exists
    if [[ -f "$model_path" ]]; then
        cp "$model_path" "$BACKUP_DIR/${model_name}.backup.$(date +%Y%m%d_%H%M%S)"
        log_info "Backed up existing $model_name"
    fi

    # Download with retry
    local max_retries=3
    local retry_count=0
    local success=false

    while [[ $retry_count -lt $max_retries && $success == false ]]; do
        if curl -L --fail --silent --show-error --retry 3 --retry-delay 2 \
               -o "$model_path.tmp" "$model_url"; then
            success=true
        else
            retry_count=$((retry_count + 1))
            if [[ $retry_count -lt $max_retries ]]; then
                log_warn "Download failed, retrying ($retry_count/$max_retries)..."
                sleep 2
            fi
        fi
    done

    if [[ $success == false ]]; then
        log_error "Failed to download $model_name after $max_retries attempts"
        return 1
    fi

    # Move temporary file to final location
    mv "$model_path.tmp" "$model_path"

    # Verify downloaded model
    if verify_model "$model_name" "$expected_size"; then
        log_success "Downloaded and verified $model_name"
        return 0
    else
        log_error "Downloaded model failed verification: $model_name"
        return 1
    fi
}

# Main download function
download_models() {
    local failed_models=()

    for model_info in "${MODEL_INFO[@]}"; do
        IFS='|' read -r model_name model_url expected_size precision speed <<< "$model_info"

        if [[ $VERIFY_ONLY == true ]]; then
            if ! verify_model "$model_name" "$expected_size"; then
                failed_models+=("$model_name")
            fi
        else
            if ! download_model "$model_name" "$model_url" "$expected_size"; then
                failed_models+=("$model_name")
            fi
        fi
    done

    return ${#failed_models[@]}
}

# Generate checksums file
generate_checksums() {
    log_info "Generating checksums..."
    (cd "$MODEL_DIR" && find . -name "*.onnx" -type f -exec sha256sum {} \;) > "$MODEL_DIR/checksums.sha256"
    log_success "Checksums saved to $MODEL_DIR/checksums.sha256"
}

# Print summary
print_summary() {
    echo
    echo "================================================================================"
    echo "🎯 YuNet Face Detection Models Summary"
    echo "================================================================================"
    echo
    echo "📁 Install Location: $MODEL_DIR"
    echo
    echo "📊 Model Performance Comparison:"
    echo "  • Original (float32):    ~25ms/frame, 0.8844 AP (RECOMMENDED - most compatible)"
    echo "  • Int8 Quantized:        ~8ms/frame, 0.8810 AP (requires ONNX quantization support)"
    echo "  • Block-Quantized:       ~6ms/frame, 0.8845 AP (requires ONNX quantization support)"
    echo
    echo "🔧 Integration:"
    echo "  • Models are automatically discovered by the Rust code"
    echo "  • Priority: float32 → int8 → int8bq (compatibility first)"
    echo "  • Docker builds COPY models from this directory"
    echo
    echo "✅ Setup complete! YuNet face detection models are ready."
    echo "================================================================================"
}

# Main execution
main() {
    echo "================================================================================"
    echo "📦 YuNet Face Detection Model Downloader"
    echo "================================================================================"
    echo

    create_directories

    if [[ $VERIFY_ONLY == true ]]; then
        log_info "Running in verify-only mode..."
        if download_models; then
            log_success "All models verified successfully"
        else
            log_error "Some models failed verification"
            exit 1
        fi
    else
        log_info "Downloading YuNet models to $MODEL_DIR..."
        echo

        if download_models; then
            generate_checksums
            print_summary
        else
            log_error "Some models failed to download or verify"
            log_info "You can retry with: $0"
            exit 1
        fi
    fi
}

# Run main function
main "$@"
