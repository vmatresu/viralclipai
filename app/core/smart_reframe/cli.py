#!/usr/bin/env python3
"""
CLI interface for the smart reframe pipeline.

Usage:
    viralclip-intelligent-crop --input video.mp4 --aspect 9:16 --output-prefix output

    # Or using Python module:
    python -m app.core.smart_reframe.cli --input video.mp4 --aspect 9:16
"""

import argparse
import json
import logging
import sys
from pathlib import Path
from typing import Optional

from app.core.smart_reframe.models import AspectRatio, CropPlan
from app.core.smart_reframe.config import (
    IntelligentCropConfig,
    DetectorBackend,
    FallbackPolicy,
    FAST_CONFIG,
    QUALITY_CONFIG,
    TIKTOK_CONFIG,
)
from app.core.smart_reframe.reframer import Reframer


def setup_logging(verbose: bool = False, json_logs: bool = False):
    """Configure logging for CLI usage."""
    level = logging.DEBUG if verbose else logging.INFO
    
    if json_logs:
        # JSON format for integration with other tools
        format_str = json.dumps({
            "time": "%(asctime)s",
            "level": "%(levelname)s",
            "module": "%(name)s",
            "message": "%(message)s",
        })
    else:
        format_str = "%(asctime)s [%(levelname)s] %(message)s"
    
    logging.basicConfig(
        level=level,
        format=format_str,
        datefmt="%Y-%m-%d %H:%M:%S",
    )


def parse_aspect_ratio(s: str) -> AspectRatio:
    """Parse aspect ratio from string like '9:16' or '9x16'."""
    return AspectRatio.from_string(s)


def parse_time(s: str) -> float:
    """Parse time from string (seconds or HH:MM:SS format)."""
    if ":" in s:
        parts = s.split(":")
        if len(parts) == 3:
            h, m, sec = parts
            return int(h) * 3600 + int(m) * 60 + float(sec)
        elif len(parts) == 2:
            m, sec = parts
            return int(m) * 60 + float(sec)
    return float(s)


def get_preset_config(preset: str) -> IntelligentCropConfig:
    """Get a preset configuration."""
    presets = {
        "fast": FAST_CONFIG,
        "quality": QUALITY_CONFIG,
        "tiktok": TIKTOK_CONFIG,
        "default": IntelligentCropConfig(),
    }
    return presets.get(preset, IntelligentCropConfig())


def main(args: Optional[list[str]] = None) -> int:
    """Main CLI entry point."""
    parser = argparse.ArgumentParser(
        description="Intelligent video reframing for portrait/square formats",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Basic usage - reframe to 9:16 (TikTok/Reels)
  %(prog)s --input video.mp4 --aspect 9:16

  # Multiple aspect ratios
  %(prog)s --input video.mp4 --aspect 9:16 --aspect 4:5 --aspect 1:1

  # Process a specific time range
  %(prog)s --input video.mp4 --aspect 9:16 --time-start 10 --time-end 60

  # Use a preset configuration
  %(prog)s --input video.mp4 --aspect 9:16 --preset tiktok

  # Custom configuration
  %(prog)s --input video.mp4 --aspect 9:16 --fps-sample 5 --max-pan-speed 150

  # Save crop plan for later re-rendering
  %(prog)s --input video.mp4 --aspect 9:16 --dump-crop-plan plan.json

  # Render from existing crop plan
  %(prog)s --input video.mp4 --load-crop-plan plan.json --output-prefix output
        """,
    )

    # Input/output
    parser.add_argument(
        "--input", "-i",
        required=True,
        help="Path to input video file",
    )
    parser.add_argument(
        "--output-prefix", "-o",
        help="Prefix for output files (default: input_reframed)",
    )
    parser.add_argument(
        "--aspect", "-a",
        type=parse_aspect_ratio,
        action="append",
        dest="aspects",
        help="Target aspect ratio (e.g., 9:16, 4:5, 1:1). Can be specified multiple times.",
    )

    # Time range
    parser.add_argument(
        "--time-start", "-ss",
        type=parse_time,
        help="Start time (seconds or HH:MM:SS)",
    )
    parser.add_argument(
        "--time-end", "-to",
        type=parse_time,
        help="End time (seconds or HH:MM:SS)",
    )

    # Presets and configuration
    parser.add_argument(
        "--preset",
        choices=["default", "fast", "quality", "tiktok"],
        default="default",
        help="Configuration preset (default: default)",
    )
    parser.add_argument(
        "--fps-sample",
        type=float,
        help="Analysis sample rate in FPS (default: 3)",
    )
    parser.add_argument(
        "--max-pan-speed",
        type=float,
        help="Maximum virtual camera pan speed in pixels/second (default: 200)",
    )
    parser.add_argument(
        "--detector",
        choices=["mediapipe", "yolo"],
        help="Detection backend (default: mediapipe)",
    )
    parser.add_argument(
        "--min-confidence",
        type=float,
        help="Minimum detection confidence (default: 0.5)",
    )
    parser.add_argument(
        "--headroom",
        type=float,
        help="Target headroom ratio (default: 0.15)",
    )
    parser.add_argument(
        "--crf",
        type=int,
        help="FFmpeg CRF quality (0-51, lower=better, default: 20)",
    )
    parser.add_argument(
        "--preset-encode",
        help="FFmpeg encoding preset (default: veryfast)",
    )

    # Crop plan
    parser.add_argument(
        "--dump-crop-plan",
        help="Save crop plan to JSON file",
    )
    parser.add_argument(
        "--load-crop-plan",
        help="Load crop plan from JSON file (skip analysis)",
    )

    # Output
    parser.add_argument(
        "--resolution",
        help="Output resolution as WxH (e.g., 1080x1920)",
    )
    parser.add_argument(
        "--crop-mode",
        choices=["intelligent"],
        default="intelligent",
        help="Crop mode (only 'intelligent' supported)",
    )

    # Logging
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Enable verbose logging",
    )
    parser.add_argument(
        "--json-logs",
        action="store_true",
        help="Output logs in JSON format",
    )
    parser.add_argument(
        "--quiet", "-q",
        action="store_true",
        help="Suppress all output except errors",
    )

    parsed = parser.parse_args(args)

    # Setup logging
    if parsed.quiet:
        logging.basicConfig(level=logging.ERROR)
    else:
        setup_logging(parsed.verbose, parsed.json_logs)

    logger = logging.getLogger("smart_reframe.cli")

    try:
        # Validate input
        input_path = Path(parsed.input)
        if not input_path.exists():
            logger.error(f"Input file not found: {input_path}")
            return 1

        # Build configuration
        config = get_preset_config(parsed.preset)

        # Override with CLI options
        if parsed.fps_sample is not None:
            config.fps_sample = parsed.fps_sample
        if parsed.max_pan_speed is not None:
            config.max_pan_speed = parsed.max_pan_speed
        if parsed.detector is not None:
            config.detector_backend = DetectorBackend(parsed.detector)
        if parsed.min_confidence is not None:
            config.min_detection_confidence = parsed.min_confidence
        if parsed.headroom is not None:
            config.headroom_ratio = parsed.headroom
        if parsed.crf is not None:
            config.render_crf = parsed.crf
        if parsed.preset_encode is not None:
            config.render_preset = parsed.preset_encode

        # Parse aspect ratios
        aspects = parsed.aspects or [AspectRatio(width=9, height=16)]

        # Parse output resolution
        output_resolution = None
        if parsed.resolution:
            parts = parsed.resolution.lower().split("x")
            if len(parts) == 2:
                output_resolution = (int(parts[0]), int(parts[1]))

        # Parse time range
        time_range = None
        if parsed.time_start is not None or parsed.time_end is not None:
            start = parsed.time_start or 0.0
            end = parsed.time_end or float("inf")
            time_range = (start, end)

        # Determine output prefix
        output_prefix = parsed.output_prefix
        if output_prefix is None:
            output_prefix = str(input_path.parent / f"{input_path.stem}_reframed")

        # Create reframer
        reframer = Reframer(
            target_aspect_ratios=aspects,
            config=config,
        )

        try:
            if parsed.load_crop_plan:
                # Load existing crop plan
                logger.info(f"Loading crop plan from: {parsed.load_crop_plan}")
                crop_plan = CropPlan.from_json_file(parsed.load_crop_plan)
                
                # Render
                output_paths = reframer.render(
                    input_path=str(input_path),
                    crop_plan=crop_plan,
                    output_prefix=output_prefix,
                    output_resolution=output_resolution,
                )
            else:
                # Analyze and render
                crop_plan = reframer.analyze(
                    input_path=str(input_path),
                    time_range=time_range,
                )

                # Save crop plan if requested
                if parsed.dump_crop_plan:
                    crop_plan.to_json_file(parsed.dump_crop_plan)
                    logger.info(f"Saved crop plan to: {parsed.dump_crop_plan}")

                # Render
                output_paths = reframer.render(
                    input_path=str(input_path),
                    crop_plan=crop_plan,
                    output_prefix=output_prefix,
                    output_resolution=output_resolution,
                )

            # Print results
            if not parsed.quiet:
                print("\nOutput files:")
                for aspect, path in output_paths.items():
                    print(f"  {aspect}: {path}")

            return 0

        finally:
            reframer.close()

    except KeyboardInterrupt:
        logger.info("Interrupted by user")
        return 130
    except Exception as e:
        logger.error(f"Error: {e}")
        if parsed.verbose:
            import traceback
            traceback.print_exc()
        return 1


if __name__ == "__main__":
    sys.exit(main())
