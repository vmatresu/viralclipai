"""
Prompt resolution utilities for video processing workflow.
"""

from typing import Optional

from app.config import PROMPT_PATH


def resolve_prompt(custom_prompt: Optional[str]) -> str:
    """
    Resolve the base prompt using priority order:
    1. User-provided custom prompt
    2. Global admin-configured prompt in Firestore
    3. Local prompt.txt fallback

    Args:
        custom_prompt: Optional user-provided custom prompt

    Returns:
        Resolved base prompt string

    Raises:
        RuntimeError: If no valid prompt can be found
    """
    if custom_prompt and custom_prompt.strip():
        return custom_prompt.strip()

    # Lazy import to avoid circular dependency
    from app.core import saas
    global_prompt = saas.get_global_prompt()
    if global_prompt:
        return global_prompt

    if not PROMPT_PATH.exists():
        raise RuntimeError(f"prompt.txt not found at {PROMPT_PATH}")

    return PROMPT_PATH.read_text(encoding="utf-8")
