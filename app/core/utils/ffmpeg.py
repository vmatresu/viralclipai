"""
FFmpeg utility functions with improved error handling and logging.

This module provides secure, production-ready FFmpeg execution with
proper error handling and AV1 warning suppression.
"""

import logging
import subprocess
import re
from typing import Optional

logger = logging.getLogger(__name__)

# Patterns for known benign warnings that should be filtered
BENIGN_WARNING_PATTERNS = [
    r"\[av1 @ .*\] Your platform doesn't suppport hardware accelerated AV1 decoding",
    r"\[av1 @ .*\] Failed to get pixel format",
    r"\[av1 @ .*\] Missing Sequence Header",
    r"\[.*\] .* does not support hardware acceleration",
    r"\[.*\] .* hardware acceleration disabled",
]


def filter_benign_warnings(stderr: str) -> tuple[str, list[str]]:
    """
    Filter out known benign warnings from FFmpeg stderr.
    
    Args:
        stderr: Raw stderr output from FFmpeg.
        
    Returns:
        Tuple of (filtered_stderr, filtered_warnings_list).
    """
    lines = stderr.split("\n")
    filtered_lines = []
    filtered_warnings = []
    
    for line in lines:
        is_benign = False
        for pattern in BENIGN_WARNING_PATTERNS:
            if re.search(pattern, line, re.IGNORECASE):
                is_benign = True
                filtered_warnings.append(line)
                break
        
        if not is_benign:
            filtered_lines.append(line)
    
    return "\n".join(filtered_lines), filtered_warnings


def run_ffmpeg(
    cmd: list[str],
    suppress_warnings: bool = True,
    log_level: str = "warning",
    check: bool = True,
) -> subprocess.CompletedProcess:
    """
    Run FFmpeg command with improved error handling.
    
    Args:
        cmd: FFmpeg command as list of arguments.
        suppress_warnings: If True, suppress AV1 and other benign warnings.
        log_level: FFmpeg log level (quiet, panic, fatal, error, warning, info, verbose, debug).
        check: If True, raise exception on non-zero exit code.
        
    Returns:
        CompletedProcess instance.
        
    Raises:
        RuntimeError: If FFmpeg fails and check=True.
    """
    # Add log level to suppress verbose output
    if suppress_warnings:
        # Insert log level after 'ffmpeg' and before other args
        # Use 'error' level to suppress warnings but keep errors
        if "-loglevel" not in cmd and log_level not in cmd:
            # Find position after 'ffmpeg' or '-y'
            insert_pos = 1
            if cmd[0] == "ffmpeg" and len(cmd) > 1:
                if cmd[1] == "-y":
                    insert_pos = 2
                else:
                    insert_pos = 1
            cmd = cmd[:insert_pos] + ["-loglevel", log_level] + cmd[insert_pos:]
    
    try:
        result = subprocess.run(
            cmd,
            check=check,
            capture_output=True,
            text=True,
        )
        
        # Filter stderr for benign warnings
        if suppress_warnings and result.stderr:
            filtered_stderr, warnings = filter_benign_warnings(result.stderr)
            if warnings:
                logger.debug(f"Filtered {len(warnings)} benign FFmpeg warnings")
            result.stderr = filtered_stderr
        
        return result
        
    except subprocess.CalledProcessError as e:
        # Filter stderr before raising
        if suppress_warnings and e.stderr:
            filtered_stderr, warnings = filter_benign_warnings(e.stderr)
            if warnings:
                logger.debug(f"Filtered {len(warnings)} benign FFmpeg warnings")
            e.stderr = filtered_stderr
        
        logger.error(f"FFmpeg failed: {e.stderr}")
        raise RuntimeError(f"FFmpeg command failed: {e.stderr}") from e

