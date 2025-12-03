#!/usr/bin/env python3
"""
Migration Script: Upload prompt.txt to Firestore

This script reads the local prompt.txt file and uploads it to Firestore
as the global admin prompt at admin/config.base_prompt.

The script will only upload if no prompt exists in Firestore. Firestore is
the source of truth for prompts - if a prompt already exists there, this
script will not overwrite it.

Usage:
    python scripts/migrate_prompt.py
"""

import sys
from pathlib import Path

# Add parent directory to path to import app modules
sys.path.insert(0, str(Path(__file__).parent.parent))

from app.config import PROMPT_PATH, logger
from app.core.saas import get_global_prompt, set_global_prompt


def main():
    """Run prompt migration."""
    logger.info("Starting prompt migration...")

    # Check if prompt already exists in Firestore
    existing_prompt = get_global_prompt()
    if existing_prompt:
        logger.info("A global prompt already exists in Firestore. Firestore is the source of truth.")
        logger.info(f"Existing prompt preview: {existing_prompt[:100]}...")
        logger.info("Skipping migration - existing prompt will be used.")
        return 0

    # Check if prompt.txt exists
    if not PROMPT_PATH.exists():
        logger.error(f"prompt.txt not found at {PROMPT_PATH}")
        logger.error("Cannot migrate: no prompt in Firestore and no local prompt.txt")
        return 1

    # Read prompt.txt
    try:
        prompt_content = PROMPT_PATH.read_text(encoding="utf-8").strip()
        if not prompt_content:
            logger.error("prompt.txt is empty")
            return 1
        logger.info(f"Read prompt.txt ({len(prompt_content)} characters)")
    except Exception as e:
        logger.error(f"Failed to read prompt.txt: {e}", exc_info=True)
        return 1

    # Upload to Firestore
    # Note: We use a system user ID for migration scripts
    # In production, this would typically be done by an admin user via the UI
    migration_uid = "system:migration"
    try:
        set_global_prompt(migration_uid, prompt_content)
        logger.info("Successfully uploaded prompt to Firestore at admin/config.base_prompt")
        
        # Verify it was saved
        verified = get_global_prompt()
        if verified == prompt_content:
            logger.info("✓ Prompt verified in Firestore")
        else:
            logger.warning("Prompt saved but verification failed (content mismatch)")
        
        return 0
    except Exception as e:
        logger.error(f"Failed to upload prompt to Firestore: {e}", exc_info=True)
        return 1


if __name__ == "__main__":
    sys.exit(main())

