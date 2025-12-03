"""
Video processing workflow package.

This package orchestrates the main video processing pipeline with:
- Parallel execution of independent operations
- Configurable concurrency limits
- Comprehensive error handling and recovery
- Progress tracking and WebSocket communication
- Resource cleanup and management

Module Structure:
- context.py: Data structures (ProcessingContext, ClipTask)
- prompt.py: Prompt resolution utilities
- data_processor.py: Data normalization and extraction
- validators.py: Style and plan validation
- parallel.py: Parallel operations (metadata, AI, download)
- clip_processor.py: Clip rendering and processing
- processor.py: Main workflow orchestration
"""

# Data structures
from app.core.workflow.context import (
    ClipTask,
    ProcessingContext,
)

# Prompt resolution
from app.core.workflow.prompt import resolve_prompt

# Data processing
from app.core.workflow.data_processor import (
    normalize_video_title,
    extract_highlights,
    normalize_highlights,
)

# Validation
from app.core.workflow.validators import (
    resolve_styles,
    validate_plan_limits,
)

# Parallel operations
from app.core.workflow.parallel import (
    fetch_video_metadata,
    run_ai_analysis,
    download_video,
    execute_parallel_operations,
)

# Clip processing
from app.core.workflow.clip_processor import (
    create_clip_tasks,
    calculate_max_workers,
    process_clip,
    process_clips_parallel,
)

# Main workflow
from app.core.workflow.processor import process_video_workflow

__all__ = [
    # Data structures
    "ClipTask",
    "ProcessingContext",
    # Prompt
    "resolve_prompt",
    # Data processing
    "normalize_video_title",
    "extract_highlights",
    "normalize_highlights",
    # Validation
    "resolve_styles",
    "validate_plan_limits",
    # Parallel operations
    "fetch_video_metadata",
    "run_ai_analysis",
    "download_video",
    "execute_parallel_operations",
    # Clip processing
    "create_clip_tasks",
    "calculate_max_workers",
    "process_clip",
    "process_clips_parallel",
    # Main workflow
    "process_video_workflow",
]

