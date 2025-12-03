"""
Validation utilities for video workflow.

Handles validation of styles, plan limits, and other workflow inputs
following Single Responsibility Principle.
"""

import logging
from typing import List, Optional

from app.config import DEFAULT_STYLE
from app.core import clipper, saas

logger = logging.getLogger(__name__)


def resolve_styles(styles: List[str]) -> List[str]:
    """
    Resolve and normalize style list, handling 'all' special case.

    Args:
        styles: List of style strings

    Returns:
        Normalized list of unique styles
    """
    styles_to_process: List[str] = []

    for style in styles:
        if style == "all":
            # "all" means include all available styles
            styles_to_process.extend(
                clipper.AVAILABLE_STYLES
                + ["intelligent", "intelligent_split", "original"]
            )
        elif style not in styles_to_process:
            styles_to_process.append(style)

    # Remove duplicates while preserving order
    seen: set = set()
    unique_styles: List[str] = []
    for style in styles_to_process:
        if style not in seen:
            seen.add(style)
            unique_styles.append(style)

    # Ensure at least one style is selected
    if not unique_styles:
        unique_styles = [DEFAULT_STYLE]

    return unique_styles


def validate_plan_limits(
    user_id: Optional[str],
    total_clips: int,
) -> None:
    """
    Validate user plan limits.

    Args:
        user_id: User ID (None for anonymous)
        total_clips: Total number of clips to be generated

    Raises:
        ValueError: If plan limits are exceeded
    """
    if user_id is None or total_clips == 0:
        return

    plan_id, max_clips = saas.get_plan_limits_for_user(user_id)
    used = saas.get_monthly_usage(user_id)

    if used + total_clips > max_clips:
        msg = (
            f"Plan limit reached for user {user_id}: "
            f"plan={plan_id}, used={used}, requested={total_clips}, max={max_clips}"
        )
        logger.warning(msg)
        raise ValueError("Clip limit reached for your current plan.")

