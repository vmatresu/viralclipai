"""
Configuration factory for optimized intelligent cropping presets.

This module provides factory functions to create optimized configurations
for different use cases, prioritizing speed, quality, or balance.
"""

import os
import logging
from typing import Optional

from app.core.smart_reframe.config import (
    IntelligentCropConfig,
    FAST_CONFIG,
    QUALITY_CONFIG,
    TIKTOK_CONFIG,
)

logger = logging.getLogger(__name__)


def get_production_config() -> IntelligentCropConfig:
    """
    Get optimized production configuration for ViralClipAI.
    
    This config balances speed and quality for production use.
    Optimized for cloud VMs with limited CPU resources.
    
    Returns:
        Optimized IntelligentCropConfig instance.
    """
    # Use FAST_CONFIG as base but with production tweaks
    config = FAST_CONFIG.model_copy(deep=True)
    
    # Override with production-optimized settings
    config.fps_sample = 1.5  # Reduced from 2.0 for faster processing
    config.analysis_resolution = 360  # Lower resolution for speed
    config.render_preset = "veryfast"  # Faster encoding
    config.render_crf = 20  # Slightly better quality than FAST_CONFIG
    config.num_workers = _get_optimal_workers()  # Parallel processing
    
    # Performance optimizations
    config.min_detection_confidence = 0.4  # Lower threshold for faster detection
    config.shot_threshold = 0.5  # Slightly higher to reduce false positives
    
    return config


def get_fast_config() -> IntelligentCropConfig:
    """
    Get fastest configuration (lowest quality).
    
    Returns:
        Fastest IntelligentCropConfig instance.
    """
    config = FAST_CONFIG.model_copy(deep=True)
    config.fps_sample = 1.0  # Even fewer samples
    config.analysis_resolution = 240  # Minimum resolution
    config.render_preset = "ultrafast"
    config.render_crf = 23  # Lower quality, faster encoding
    config.num_workers = _get_optimal_workers()
    return config


def get_balanced_config() -> IntelligentCropConfig:
    """
    Get balanced configuration (good speed/quality tradeoff).
    
    Returns:
        Balanced IntelligentCropConfig instance.
    """
    config = get_production_config()
    config.fps_sample = 2.0  # More samples for better quality
    config.analysis_resolution = 480  # Higher resolution
    config.render_preset = "fast"
    config.render_crf = 18  # Better quality
    return config


def get_quality_config() -> IntelligentCropConfig:
    """
    Get high-quality configuration (slower but better results).
    
    Returns:
        Quality IntelligentCropConfig instance.
    """
    config = QUALITY_CONFIG.model_copy(deep=True)
    config.num_workers = _get_optimal_workers()
    return config


def _get_optimal_workers() -> int:
    """
    Determine optimal number of worker processes.
    
    Returns:
        Number of workers (1-4, based on CPU count).
    """
    try:
        import multiprocessing
        cpu_count = multiprocessing.cpu_count()
        # Use up to 4 workers, but not more than CPU count
        workers = min(4, max(1, cpu_count - 1))  # Leave one CPU free
        return workers
    except Exception:
        return 1  # Fallback to single worker


def get_config_from_env() -> IntelligentCropConfig:
    """
    Get configuration from environment variables.
    
    Environment variables:
        CROP_CONFIG_MODE: "fast", "balanced", "production", or "quality"
        CROP_FPS_SAMPLE: Override fps_sample (float)
        CROP_ANALYSIS_RES: Override analysis_resolution (int)
        CROP_RENDER_PRESET: Override render_preset (str)
        CROP_NUM_WORKERS: Override num_workers (int)
    
    Returns:
        Configured IntelligentCropConfig instance.
    """
    mode = os.getenv("CROP_CONFIG_MODE", "production").lower()
    
    # Get base config based on mode
    if mode == "fast":
        config = get_fast_config()
    elif mode == "balanced":
        config = get_balanced_config()
    elif mode == "quality":
        config = get_quality_config()
    else:  # production or default
        config = get_production_config()
    
    # Override with environment variables if set
    if fps_sample := os.getenv("CROP_FPS_SAMPLE"):
        try:
            config.fps_sample = float(fps_sample)
        except ValueError:
            logger.warning(f"Invalid CROP_FPS_SAMPLE: {fps_sample}")
    
    if analysis_res := os.getenv("CROP_ANALYSIS_RES"):
        try:
            config.analysis_resolution = int(analysis_res)
        except ValueError:
            logger.warning(f"Invalid CROP_ANALYSIS_RES: {analysis_res}")
    
    if render_preset := os.getenv("CROP_RENDER_PRESET"):
        config.render_preset = render_preset
    
    if num_workers := os.getenv("CROP_NUM_WORKERS"):
        try:
            config.num_workers = int(num_workers)
        except ValueError:
            logger.warning(f"Invalid CROP_NUM_WORKERS: {num_workers}")
    
    return config

